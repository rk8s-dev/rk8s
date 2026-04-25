use crate::error::{AppError, HeaderError, OciError};
use crate::service::manifest::{delete_blob_if_unreferenced, purge_repo_tags_for_digest};
use crate::utils::state::AppState;
use crate::utils::validation::is_valid_name;
use axum::body::Body;
use axum::extract::{Query, Request, State};
use axum::http::header::{HeaderMap, LOCATION, RANGE};
use axum::http::{Response, header};
use axum::response::IntoResponse;
use axum::{extract::Path, http::StatusCode};
use oci_spec::image::Digest as oci_digest;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

pub async fn get_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((name, digest_str)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }

    let digest = oci_digest::from_str(&digest_str)
        .map_err(|_| OciError::DigestInvalid(digest_str.clone()))?;

    let total_size = state.storage.blob_size(&digest).await?;
    let range = match parse_blob_range(&headers, total_size) {
        BlobRangeRequest::None => {
            let obj = state.storage.get_blob(&digest).await?;
            let body = Body::from_stream(obj.stream);

            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::CONTENT_LENGTH, obj.size)
                .header("Accept-Ranges", "bytes")
                .header("Docker-Content-Digest", digest_str)
                .body(body)
                .unwrap());
        }
        BlobRangeRequest::Satisfiable(range) => range,
        BlobRangeRequest::Unsatisfiable => {
            return Ok(range_not_satisfiable_response(total_size));
        }
    };

    let obj = state
        .storage
        .get_blob_range(&digest, range.start..range.end + 1)
        .await?;
    let body = Body::from_stream(obj.stream);

    Ok(Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, obj.size)
        .header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", range.start, range.end, range.total),
        )
        .header("Accept-Ranges", "bytes")
        .header("Docker-Content-Digest", digest_str)
        .body(body)
        .unwrap())
}

pub async fn head_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((name, digest_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }

    let digest = oci_digest::from_str(&digest_str)
        .map_err(|_| OciError::DigestInvalid(digest_str.clone()))?;

    let content_length = state.storage.blob_size(&digest).await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, content_length)
        .header("Accept-Ranges", "bytes")
        .header("Docker-Content-Digest", digest_str)
        .body(Body::empty())
        .unwrap())
}

pub async fn post_blob_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    request: Request,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }

    if let Some(mount_digest_str) = params.get("mount") {
        let digest = oci_digest::from_str(mount_digest_str)
            .map_err(|_| OciError::DigestInvalid(mount_digest_str.clone()))?;

        if state.storage.blob_exists(&digest).await? {
            let location = format!("/v2/{name}/blobs/{digest}");
            return Ok(Response::builder()
                .status(StatusCode::CREATED)
                .header(LOCATION, location)
                .header("Docker-Content-Digest", digest.to_string())
                .body(Body::empty())
                .unwrap());
        }

        let session_id = state.create_session().await;
        let location = format!("/v2/{name}/blobs/uploads/{session_id}");
        return Ok(Response::builder()
            .status(StatusCode::ACCEPTED)
            .header(LOCATION, location)
            .header("Docker-Upload-UUID", &session_id)
            .body(Body::empty())
            .unwrap());
    }

    if let Some(digest_str) = params.get("digest") {
        let content_length = headers
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or_else(|| {
                OciError::SizeInvalid(
                    "Content-Length header is required for monolithic upload".to_string(),
                )
            })?;

        if content_length == 0 {
            return Err(OciError::SizeInvalid("Content-Length cannot be zero".to_string()).into());
        }

        let digest = oci_digest::from_str(digest_str)
            .map_err(|_| OciError::DigestInvalid(digest_str.clone()))?;

        state
            .storage
            .put_blob(&digest, request.into_body().into_data_stream())
            .await?;

        let location = format!("/v2/{name}/blobs/{digest}");
        return Ok(Response::builder()
            .status(StatusCode::CREATED)
            .header(LOCATION, location)
            .header("Docker-Content-Digest", digest.to_string())
            .body(Body::empty())
            .unwrap());
    }

    let session_id = state.create_session().await;
    let location = format!("/v2/{name}/blobs/uploads/{session_id}");
    Ok(Response::builder()
        .status(StatusCode::ACCEPTED)
        .header(LOCATION, location)
        .header("Docker-Upload-UUID", session_id)
        .body(Body::empty())
        .unwrap())
}

pub async fn patch_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((name, session_id)): Path<(String, String)>,
    headers: HeaderMap,
    request: Request,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }

    let session_lock = state
        .get_session_lock(&session_id)
        .await
        .ok_or_else(|| OciError::BlobUploadUnknown(session_id.clone()))?;
    let _session_guard = session_lock.lock().await;

    let session = state
        .get_session(&session_id)
        .await
        .ok_or_else(|| OciError::BlobUploadUnknown(session_id.clone()))?;

    if headers.get(header::CONTENT_RANGE).is_some() {
        let (start_offset, _) = parse_content_range(&headers)?;
        let current_uploaded_bytes = session.uploaded;
        if start_offset != current_uploaded_bytes {
            return Err(HeaderError::RangeNotSatisfiable {
                session_id,
                name,
                current_size: current_uploaded_bytes,
            }
            .into());
        }
    }

    let n_bytes = state
        .storage
        .write_upload_chunk(&session_id, request.into_body().into_data_stream())
        .await?;

    let new_total_size = state
        .update_session(&session_id, n_bytes)
        .await
        .ok_or_else(|| OciError::BlobUploadUnknown(session_id.clone()))?;

    let location = format!("/v2/{name}/blobs/uploads/{session_id}");
    let range = upload_range_header(new_total_size);

    let mut builder = Response::builder()
        .status(StatusCode::ACCEPTED)
        .header(LOCATION, location)
        .header("Docker-Upload-UUID", &session_id);
    if let Some(range) = range {
        builder = builder.header(RANGE, range);
    }
    Ok(builder.body(Body::empty()).unwrap())
}

pub async fn put_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((name, session_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    request: Request,
) -> Result<impl IntoResponse, AppError> {
    let session_lock = state
        .get_session_lock(&session_id)
        .await
        .ok_or_else(|| OciError::BlobUploadUnknown(session_id.clone()))?;
    let _session_guard = session_lock.lock().await;

    state
        .get_session(&session_id)
        .await
        .ok_or_else(|| OciError::BlobUploadUnknown(session_id.clone()))?;

    let digest_str = params.get("digest").ok_or_else(|| {
        OciError::DigestInvalid("digest query parameter is required to finalize upload".to_string())
    })?;

    let digest = oci_digest::from_str(digest_str)
        .map_err(|_| OciError::DigestInvalid(digest_str.clone()))?;

    let body = request.into_body().into_data_stream();

    state.storage.write_upload_chunk(&session_id, body).await?;
    let finalize_result = state.storage.finalize_upload(&session_id, &digest).await;
    state.close_session(&session_id).await;
    finalize_result?;

    let location = format!("/v2/{name}/blobs/{digest}");
    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header(LOCATION, location)
        .header("Docker-Content-Digest", digest.to_string())
        .body(Body::empty())
        .unwrap())
}

pub async fn get_blob_status_handler(
    State(state): State<Arc<AppState>>,
    Path((name, session_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let session_lock = state
        .get_session_lock(&session_id)
        .await
        .ok_or_else(|| OciError::BlobUploadUnknown(session_id.clone()))?;
    let _session_guard = session_lock.lock().await;

    if let Some(session) = state.get_session(&session_id).await {
        let location = format!("/v2/{name}/blobs/uploads/{session_id}");
        let range = upload_range_header(session.uploaded);

        let mut builder = Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(LOCATION, location)
            .header("Docker-Upload-UUID", &session_id);
        if let Some(range) = range {
            builder = builder.header(RANGE, range);
        }
        Ok(builder.body(Body::empty()).unwrap())
    } else {
        Err(OciError::BlobUploadUnknown(session_id).into())
    }
}

pub async fn delete_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((name, digest_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let digest =
        oci_digest::from_str(&digest_str).map_err(|_| OciError::DigestInvalid(digest_str))?;

    purge_repo_tags_for_digest(&state, &name, &digest).await?;
    delete_blob_if_unreferenced(&state, &digest).await?;

    Ok(Response::builder()
        .status(StatusCode::ACCEPTED)
        .body(Body::empty())
        .unwrap())
}

fn parse_content_range(headers: &HeaderMap) -> Result<(u64, u64), AppError> {
    let content_length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    if let Some(range_header) = headers
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
    {
        let parts: Vec<&str> = range_header.split('-').collect();
        if parts.len() != 2 {
            return Err(HeaderError::ContentRangeInvalid("Invalid format".to_string()).into());
        }

        let start = parts[0].parse().map_err(|_| {
            HeaderError::ContentRangeInvalid("Failed to parse start offset".to_string())
        })?;
        let end = parts[1].parse().map_err(|_| {
            HeaderError::ContentRangeInvalid("Failed to parse end offset".to_string())
        })?;
        if start > end {
            return Err(HeaderError::ContentRangeInvalid(
                "Start offset cannot be greater than end offset".to_string(),
            )
            .into());
        }

        if let Some(content_length) = content_length
            && content_length != (end - start + 1)
        {
            return Err(OciError::SizeInvalid(
                "Content-Length does not match Content-Range".to_string(),
            )
            .into());
        }

        return Ok((start, end));
    }
    if let Some(content_length) = content_length {
        if content_length > 0 {
            return Ok((0, content_length - 1));
        }
        return Err(OciError::SizeInvalid(
            "Content-Length must be greater than zero for a PATCH request without Content-Range"
                .to_string(),
        )
        .into());
    }
    Err(
        OciError::SizeInvalid("Content-Length or Content-Range header is required".to_string())
            .into(),
    )
}

#[derive(Debug, PartialEq, Eq)]
enum BlobRangeRequest {
    None,
    Satisfiable(BlobRange),
    Unsatisfiable,
}

#[derive(Debug, PartialEq, Eq)]
struct BlobRange {
    start: u64,
    end: u64,
    total: u64,
}

fn parse_blob_range(headers: &HeaderMap, total_size: u64) -> BlobRangeRequest {
    let Some(header_value) = headers.get(RANGE) else {
        return BlobRangeRequest::None;
    };
    let Ok(header_value) = header_value.to_str() else {
        return BlobRangeRequest::Unsatisfiable;
    };
    let Some((unit, range_spec)) = header_value.trim().split_once('=') else {
        return BlobRangeRequest::Unsatisfiable;
    };
    if !unit.eq_ignore_ascii_case("bytes") || range_spec.contains(',') {
        return BlobRangeRequest::Unsatisfiable;
    }

    let Some((start_spec, end_spec)) = range_spec.split_once('-') else {
        return BlobRangeRequest::Unsatisfiable;
    };
    let start_spec = start_spec.trim();
    let end_spec = end_spec.trim();

    if start_spec.is_empty() {
        return parse_blob_suffix_range(end_spec, total_size);
    }

    let Ok(start) = start_spec.parse::<u64>() else {
        return BlobRangeRequest::Unsatisfiable;
    };
    if start >= total_size {
        return BlobRangeRequest::Unsatisfiable;
    }

    let end = if end_spec.is_empty() {
        total_size - 1
    } else {
        let Ok(requested_end) = end_spec.parse::<u64>() else {
            return BlobRangeRequest::Unsatisfiable;
        };
        if requested_end < start {
            return BlobRangeRequest::Unsatisfiable;
        }
        requested_end.min(total_size - 1)
    };

    BlobRangeRequest::Satisfiable(BlobRange {
        start,
        end,
        total: total_size,
    })
}

fn parse_blob_suffix_range(end_spec: &str, total_size: u64) -> BlobRangeRequest {
    let Ok(suffix_len) = end_spec.parse::<u64>() else {
        return BlobRangeRequest::Unsatisfiable;
    };
    if suffix_len == 0 || total_size == 0 {
        return BlobRangeRequest::Unsatisfiable;
    }

    let start = total_size.saturating_sub(suffix_len);
    BlobRangeRequest::Satisfiable(BlobRange {
        start,
        end: total_size - 1,
        total: total_size,
    })
}

fn range_not_satisfiable_response(total_size: u64) -> Response<Body> {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{total_size}"))
        .header("Accept-Ranges", "bytes")
        .body(Body::empty())
        .unwrap()
}

fn upload_range_header(uploaded: u64) -> Option<String> {
    if uploaded == 0 {
        None
    } else {
        Some(format!("0-{}", uploaded - 1))
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, header};

    use super::{BlobRange, BlobRangeRequest, parse_blob_range};

    fn range_headers(range: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, range.parse().unwrap());
        headers
    }

    #[test]
    fn parse_blob_range_returns_none_without_header() {
        assert_eq!(
            parse_blob_range(&HeaderMap::new(), 10),
            BlobRangeRequest::None
        );
    }

    #[test]
    fn parse_blob_range_supports_open_ended_range() {
        assert_eq!(
            parse_blob_range(&range_headers("bytes=5-"), 10),
            BlobRangeRequest::Satisfiable(BlobRange {
                start: 5,
                end: 9,
                total: 10,
            })
        );
    }

    #[test]
    fn parse_blob_range_caps_end_at_total_size() {
        assert_eq!(
            parse_blob_range(&range_headers("bytes=5-99"), 10),
            BlobRangeRequest::Satisfiable(BlobRange {
                start: 5,
                end: 9,
                total: 10,
            })
        );
    }

    #[test]
    fn parse_blob_range_supports_suffix_range() {
        assert_eq!(
            parse_blob_range(&range_headers("bytes=-4"), 10),
            BlobRangeRequest::Satisfiable(BlobRange {
                start: 6,
                end: 9,
                total: 10,
            })
        );
    }

    #[test]
    fn parse_blob_range_rejects_unsatisfiable_range() {
        assert_eq!(
            parse_blob_range(&range_headers("bytes=10-"), 10),
            BlobRangeRequest::Unsatisfiable
        );
    }
}
