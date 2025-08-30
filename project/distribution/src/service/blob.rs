use crate::error::{AppError, HeaderError, MapToAppError, OciError};
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
use tokio_util::io::ReaderStream;

/// Handles `GET /v2/<name>/blobs/<digest>`.
///
/// **Purpose:** Downloads a blob (an image layer or config file) from the registry.
///
/// **Behavior according to OCI Distribution Spec:**
/// - Blobs are content-addressable and immutable. The `<digest>` uniquely identifies the content.
/// - If the blob exists, the server MUST return a `200 OK` with the raw binary data of the
///   blob as the response body.
/// - The `<name>` (repository) is required for authorization; the server must check if the
///   user has pull access to the repository before serving the blob.
/// - If the blob does not exist, the server MUST return `404 Not Found` with a `BLOB_UNKNOWN`
///   error code.
pub async fn get_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((name, digest_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }

    let digest = oci_digest::from_str(&digest_str)
        .map_err(|_| OciError::DigestInvalid(digest_str.clone()))?;

    let file = state.storage.read_by_digest(&digest).await?;
    let content_length = file.metadata().await.map_to_internal()?.len();
    let file_stream = ReaderStream::new(file);
    let body = Body::from_stream(file_stream);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, content_length)
        .header("Docker-Content-Digest", digest_str)
        .body(body)
        .unwrap())
}

/// Handles `HEAD /v2/<name>/blobs/<digest>`.
///
/// **Purpose:** Checks for the existence of a blob and retrieves its metadata (size)
/// without downloading the content.
///
/// **Behavior according to OCI Distribution Spec:**
/// - If the blob exists, it MUST return `200 OK` with an empty body.
/// - The response MUST include the same headers as a `GET` request, especially `Content-Length`.
/// - If the blob does not exist, it MUST return `404 Not Found`.
pub async fn head_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((name, digest_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    if !is_valid_name(&name) {
        return Err(OciError::NameInvalid(name).into());
    }

    let digest = oci_digest::from_str(&digest_str)
        .map_err(|_| OciError::DigestInvalid(digest_str.clone()))?;

    let file = state.storage.read_by_digest(&digest).await?;
    let content_length = file.metadata().await.map_to_internal()?.len();

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, content_length)
        .header("Docker-Content-Digest", digest_str)
        .body(Body::empty())
        .unwrap())
}

/// Handles `POST /v2/<name>/blobs/uploads/`.
///
/// **Purpose:** Initiates a new blob upload session or performs a single-request ("monolithic") upload.
///
/// **Behavior according to OCI Distribution Spec:**
/// - **To start a session:** Send a POST with an empty body. The server MUST return `202 Accepted`,
///   a `Location` header containing the unique session URL (including a UUID), and a `Docker-Upload-UUID` header.
/// - **For monolithic upload:** Include the `?digest=` query parameter. The request body contains the
///   entire blob content. On success, the server MUST return `201 Created`.
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
            .write_by_digest(&digest, request.into_body().into_data_stream(), false)
            .await?;

        let location = format!("/v2/{name}/blobs/{digest}");
        Ok(Response::builder()
            .status(StatusCode::CREATED)
            .header(LOCATION, location)
            .header("Docker-Content-Digest", digest.to_string())
            .body(Body::empty())
            .unwrap())
    } else {
        let session_id = state.create_session().await;
        let location = format!("/v2/{name}/blobs/uploads/{session_id}");
        Ok(Response::builder()
            .status(StatusCode::ACCEPTED)
            .header(LOCATION, location)
            .header("Docker-Upload-UUID", session_id)
            .body(Body::empty())
            .unwrap())
    }
}

/// Handles `PATCH /v2/<name>/blobs/uploads/<session_id>`.
///
/// **Purpose:** Uploads a chunk of data for a blob.
///
/// **Behavior according to OCI Distribution Spec:**
/// - The request body contains the raw binary data of the chunk.
/// - The request MUST include `Content-Length` and `Content-Range` headers.
/// - The server MUST validate that the `Content-Range` is sequential and contiguous with previously
///   uploaded chunks for the session.
/// - If a chunk is out of order, the server MUST return `416 Range Not Satisfiable`.
/// - On success, the server MUST return `202 Accepted` with a `Range` header indicating the
///   total number of bytes received so far for the upload.
pub async fn patch_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((name, session_id)): Path<(String, String)>,
    headers: HeaderMap,
    request: Request,
) -> Result<impl IntoResponse, AppError> {
    if headers.get(header::CONTENT_RANGE).is_some() {
        let (start_offset, _) = parse_content_range(&headers)?;
        let session = state
            .get_session(&session_id)
            .await
            .ok_or_else(|| OciError::BlobUploadUnknown(session_id.clone()))?;
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
        .write_by_uuid(&session_id, request.into_body().into_data_stream(), true)
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

/// Handles `PUT /v2/<name>/blobs/uploads/<session_id>`.
///
/// **Purpose:** Completes or finalizes a blob upload session.
///
/// **Behavior according to OCI Distribution Spec:**
/// - The request MUST include the `?digest=` query parameter, specifying the digest of the
///   complete blob content.
/// - The final chunk of data can optionally be included in the request body.
/// - The server MUST verify that the digest of the fully assembled blob matches the provided digest.
/// - On success, the server MUST return `201 Created` with a `Location` header pointing to the
///   blob's canonical location by its digest.
pub async fn put_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((name, session_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    request: Request,
) -> Result<impl IntoResponse, AppError> {
    let digest_str = params.get("digest").ok_or_else(|| {
        OciError::DigestInvalid("digest query parameter is required to finalize upload".to_string())
    })?;

    let digest = oci_digest::from_str(digest_str)
        .map_err(|_| OciError::DigestInvalid(digest_str.clone()))?;

    let body = request.into_body().into_data_stream();

    state.storage.write_by_uuid(&session_id, body, true).await?;
    state.storage.move_to_digest(&session_id, &digest).await?;
    state.close_session(&session_id).await;

    let location = format!("/v2/{name}/blobs/{digest}");
    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header(LOCATION, location)
        .header("Docker-Content-Digest", digest.to_string())
        .body(Body::empty())
        .unwrap())
}

/// GET /v2/<name>/blobs/uploads/<session_id>
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

/// DELETE /v2/<name>/blobs/<digest>
pub async fn delete_blob_handler(
    State(state): State<Arc<AppState>>,
    Path((_name, digest_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let digest =
        oci_digest::from_str(&digest_str).map_err(|_| OciError::DigestInvalid(digest_str))?;

    state.storage.delete_by_digest(&digest).await?;

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
