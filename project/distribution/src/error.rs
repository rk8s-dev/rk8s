use std::io;
use std::sync::Arc;
use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::http::header::{LOCATION, RANGE};
use axum::Json;
use axum::response::{IntoResponse, Response};
use oci_spec::distribution::{ErrorCode, ErrorInfo, ErrorInfoBuilder, ErrorResponseBuilder};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;
use crate::config::Config;

#[derive(Debug, Serialize, Clone)]
pub struct OciError {
    code: ErrorCode,
    message: String,
    detail: serde_json::Value,
}

impl OciError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: json!({}),
        }
    }

    pub fn with_detail(mut self, detail: serde_json::Value) -> Self {
        self.detail = detail;
        self
    }
}

impl From<OciError> for ErrorInfo {
    fn from(val: OciError) -> Self {
        ErrorInfoBuilder::default()
            .code(val.code)
            .message(val.message)
            .detail(serde_json::to_string_pretty(&val.detail).unwrap())
            .build()
            .unwrap()
    }
}

impl IntoResponse for OciError {
    fn into_response(self) -> Response {
        tracing::error!("Generating OCI error response: {:?}", self);

        let status_code = match self.code {
            ErrorCode::BlobUnknown
            | ErrorCode::BlobUploadUnknown
            | ErrorCode::ManifestUnknown
            | ErrorCode::NameUnknown => {
                StatusCode::NOT_FOUND
            }
            ErrorCode::BlobUploadInvalid
            | ErrorCode::DigestInvalid
            | ErrorCode::ManifestBlobUnknown
            | ErrorCode::ManifestInvalid
            | ErrorCode::NameInvalid
            | ErrorCode::SizeInvalid => {
                StatusCode::BAD_REQUEST
            }
            ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
            ErrorCode::Denied => StatusCode::FORBIDDEN,
            ErrorCode::Unsupported | ErrorCode::TooManyRequests => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        let error_response_body = ErrorResponseBuilder::default()
            .errors(vec![self.into()])
            .build()
            .unwrap();
        (status_code, Json(error_response_body)).into_response()
    }
}

#[derive(Error, Debug)]
pub enum AppError {
    // OCI Specific Errors (with context)
    #[error("Blob unknown: {0}")]
    BlobUnknown(String), // Contains the digest

    #[error("Blob upload invalid: {0}")]
    BlobUploadInvalid(String), // Contains a descriptive message

    #[error("Blob upload unknown: {0}")]
    BlobUploadUnknown(String), // Contains the session ID

    #[error("Digest invalid: {0}")]
    DigestInvalid(String), // Contains a descriptive message

    #[error("Manifest references an unknown blob: {0}")]
    ManifestBlobUnknown(String), // Contains the blob digest

    #[error("Manifest invalid: {0}")]
    ManifestInvalid(String), // Contains a validation error message

    #[error("Manifest unknown: {0}")]
    ManifestUnknown(String), // Contains the reference (tag or digest)

    #[error("Invalid repository name: {0}")]
    NameInvalid(String), // Contains the invalid name

    #[error("Repository not known to registry: {0}")]
    NameUnknown(String), // Contains the repository name

    #[error("Invalid content size: {0}")]
    SizeInvalid(String), // Contains a descriptive message

    #[error("The operation is unsupported")]
    Unsupported,

    #[error("Too many requests")]
    TooManyRequests,

    // --- Auth Errors (that map to OCI errors) ---
    #[error("{0}")]
    Unauthorized(String, Option<Arc<Config>>),

    #[error("{0}")]
    Forbidden(String),

    // Business Logic / Non-OCI Errors
    #[error("username {0} is already taken")]
    UsernameTaken(String),

    #[error("invalid password")]
    InvalidPassword,

    #[error("{0} not found")]
    NotFound(String),

    #[error("Content-Range header is missing")]
    ContentRangeMissing,

    #[error("Content-Range header is invalid: {0}")]
    ContentRangeInvalid(String),

    #[error("Range not satisfiable for upload {session_id}")]
    RangeNotSatisfiable {
        session_id: String,
        name: String,
        current_size: u64,
    },

    // Internal Errors
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Bcrypt error: {0}")]
    Bcrypt(#[from] bcrypt::BcryptError),

    #[error("jwt error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Axum error: {0}")]
    AuxmError(#[from] axum::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("Generating response for AppError: {:?}", self);

        if let Self::RangeNotSatisfiable {
            session_id, name, current_size
        } = self {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(LOCATION, format!("/v2/{name}/blobs/uploads/{session_id}"))
                .header(RANGE, format!("0-{}", current_size.saturating_sub(1)))
                .header("Docker-Upload-UUID", session_id)
                .body(Body::empty())
                .unwrap();
        }

        let (status_code, error_info) = match &self {
            Self::BlobUnknown(digest) =>
                (StatusCode::NOT_FOUND, oci_error(ErrorCode::BlobUnknown, "blob unknown", json!({ "digest": digest }))),
            Self::BlobUploadInvalid(msg) =>
                (StatusCode::BAD_REQUEST, oci_error(ErrorCode::BlobUploadInvalid, msg, json!({}))),
            Self::BlobUploadUnknown(session_id) =>
                (StatusCode::NOT_FOUND, oci_error(ErrorCode::BlobUploadUnknown, "blob upload unknown", json!({ "session_id": session_id }))),
            Self::DigestInvalid(msg) =>
                (StatusCode::BAD_REQUEST, oci_error(ErrorCode::DigestInvalid, msg, json!({}))),
            Self::ManifestBlobUnknown(digest) =>
                (StatusCode::BAD_REQUEST, oci_error(ErrorCode::ManifestBlobUnknown, "manifest references unknown blob", json!({ "digest": digest }))),
            Self::ManifestInvalid(msg) =>
                (StatusCode::NOT_FOUND, oci_error(ErrorCode::ManifestInvalid, msg, json!({}))),
            Self::ManifestUnknown(reference) =>
                (StatusCode::NOT_FOUND, oci_error(ErrorCode::ManifestUnknown, "manifest unknown", json!({ "reference": reference }))),
            Self::NameInvalid(name) =>
                (StatusCode::BAD_REQUEST, oci_error(ErrorCode::NameInvalid, "invalid repository name", json!({ "name": name }))),
            Self::NameUnknown(name) =>
                (StatusCode::NOT_FOUND, oci_error(ErrorCode::NameUnknown, "repository not known to registry", json!({ "name": name }))),
            Self::SizeInvalid(msg) =>
                (StatusCode::BAD_REQUEST, oci_error(ErrorCode::SizeInvalid, msg, json!({}))),
            Self::Unsupported =>
                (StatusCode::METHOD_NOT_ALLOWED, oci_error(ErrorCode::Unsupported, "operation is unsupported", json!({}))),
            Self::TooManyRequests =>
                (StatusCode::TOO_MANY_REQUESTS, oci_error(ErrorCode::TooManyRequests, "too many requests", json!({}))),
            Self::Unauthorized(msg, _) =>
                (StatusCode::UNAUTHORIZED, oci_error(ErrorCode::Unauthorized, msg, json!({}))),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, oci_error(ErrorCode::Denied, msg, json!({}))),
            Self::UsernameTaken(username) =>
                (StatusCode::CONFLICT, oci_error(ErrorCode::Denied, "username is already taken", json!({ "username": username }))),
            Self::InvalidPassword =>
                (StatusCode::BAD_REQUEST, oci_error(ErrorCode::Denied, "invalid password", json!({}))),
            Self::ContentRangeMissing =>
                (StatusCode::BAD_REQUEST, oci_error(ErrorCode::BlobUploadInvalid, "Content-Range header is required for chunked upload", json!({}))),
            Self::ContentRangeInvalid(reason) =>
                (StatusCode::BAD_REQUEST, oci_error(ErrorCode::BlobUploadInvalid, "Content-Range header is invalid", json!({ "reason": reason }))),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, oci_error(ErrorCode::Unsupported, "an internal server error occurred", json!({}))),
        };

        let error_response = ErrorResponseBuilder::default().errors(vec![error_info]).build().unwrap();

        if let Self::Unauthorized(_, Some(config)) = self {
            let realm = format!("{}/auth/token", config.registry_url);
            let challenge = format!(r#"Bearer realm="{}",service="oci-registry",scope="repository:*:*""#, realm);
            let mut headers = HeaderMap::new();
            headers.insert("Www-Authenticate", challenge.parse().unwrap());
            return (status_code, headers, Json(error_response)).into_response();
        }

        (status_code, Json(error_response)).into_response()
    }
}

fn oci_error(code: ErrorCode, message: impl Into<String>, detail: serde_json::Value) -> ErrorInfo {
    ErrorInfoBuilder::default()
        .code(code)
        .message(message.into())
        .detail(serde_json::to_string_pretty(&detail).unwrap())
        .build()
        .unwrap()
}