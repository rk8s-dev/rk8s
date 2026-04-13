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
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }

    let digest = oci_digest::from_str(&digest_str)
        .map_err(|_| OciError::DigestInvalid(digest_str.clone()))?;

    let obj = state.storage.get_blob(&digest).await?;
    let body = Body::from_stream(obj.stream);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, obj.size)
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
    let end_of_range = new_total_size.saturating_sub(1);

    Ok(Response::builder()
        .status(StatusCode::ACCEPTED)
        .header(LOCATION, location)
        .header(RANGE, format!("0-{end_of_range}"))
        .header("Docker-Upload-UUID", &session_id)
        .body(Body::empty())
        .unwrap())
}

pub async fn put_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((name, session_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    request: Request,
) -> Result<impl IntoResponse, AppError> {
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
    state.storage.finalize_upload(&session_id, &digest).await?;
    state.close_session(&session_id).await;

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
    if let Some(session) = state.get_session(&session_id).await {
        let location = format!("/v2/{name}/blobs/uploads/{session_id}");
        let end_of_range = session.uploaded.saturating_sub(1);

        Ok(Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(LOCATION, location)
            .header(RANGE, format!("0-{end_of_range}"))
            .header("Docker-Upload-UUID", &session_id)
            .body(Body::empty())
            .unwrap())
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
