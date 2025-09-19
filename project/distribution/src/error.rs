use axum::Json;
use axum::body::Body;
use axum::http::StatusCode;
use axum::http::header::{LOCATION, RANGE};
use axum::response::{IntoResponse, Response};
use oci_spec::distribution::{ErrorCode, ErrorInfo, ErrorInfoBuilder, ErrorResponseBuilder};
use serde_json::json;
use std::io;
use thiserror::Error;

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum OciError {
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

    #[error("{msg}")]
    Unauthorized {
        msg: String,
        auth_url: Option<String>,
    },

    #[error("{0}")]
    Forbidden(String),
}

impl IntoResponse for OciError {
    fn into_response(self) -> Response {
        let (status_code, error_info) = match &self {
            Self::BlobUnknown(digest) => (
                StatusCode::NOT_FOUND,
                ErrorInfo::new(ErrorCode::BlobUnknown, "blob unknown")
                    .with_detail(json!({ "digest": digest })),
            ),
            Self::BlobUploadInvalid(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorInfo::new(ErrorCode::BlobUploadInvalid, msg),
            ),
            Self::BlobUploadUnknown(session_id) => (
                StatusCode::NOT_FOUND,
                ErrorInfo::new(ErrorCode::BlobUploadUnknown, "blob upload unknown")
                    .with_detail(json!({ "session_id": session_id })),
            ),
            Self::DigestInvalid(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorInfo::new(ErrorCode::DigestInvalid, msg),
            ),
            Self::ManifestBlobUnknown(digest) => (
                StatusCode::NOT_FOUND,
                ErrorInfo::new(
                    ErrorCode::ManifestBlobUnknown,
                    "manifest references unknown blob",
                )
                .with_detail(json!({ "digest": digest })),
            ),
            Self::ManifestInvalid(msg) => (
                StatusCode::NOT_FOUND,
                ErrorInfo::new(ErrorCode::ManifestInvalid, msg),
            ),
            Self::ManifestUnknown(reference) => (
                StatusCode::NOT_FOUND,
                ErrorInfo::new(ErrorCode::ManifestUnknown, "manifest unknown")
                    .with_detail(json!({ "reference": reference })),
            ),
            Self::NameInvalid(name) => (
                StatusCode::BAD_REQUEST,
                ErrorInfo::new(ErrorCode::NameInvalid, "invalid repository name")
                    .with_detail(json!({ "name": name })),
            ),
            Self::NameUnknown(name) => (
                StatusCode::NOT_FOUND,
                ErrorInfo::new(ErrorCode::NameUnknown, "repository not known to registry")
                    .with_detail(json!({ "name": name })),
            ),
            Self::SizeInvalid(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorInfo::new(ErrorCode::SizeInvalid, msg),
            ),
            Self::Unsupported => (
                StatusCode::METHOD_NOT_ALLOWED,
                ErrorInfo::new(ErrorCode::Unsupported, "the operation is unsupported"),
            ),
            Self::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                ErrorInfo::new(ErrorCode::TooManyRequests, "too many requests"),
            ),
            Self::Unauthorized { msg, .. } => (
                StatusCode::UNAUTHORIZED,
                ErrorInfo::new(ErrorCode::Unauthorized, msg),
            ),
            Self::Forbidden(msg) => (
                StatusCode::FORBIDDEN,
                ErrorInfo::new(ErrorCode::Denied, msg),
            ),
        };

        let body = ErrorResponseBuilder::default()
            .errors(vec![error_info])
            .build()
            .unwrap();
        let mut response = (status_code, Json(body)).into_response();

        if let Self::Unauthorized { auth_url, .. } = self
            && let Some(auth_url) = auth_url
        {
            let realm = format!("{auth_url}/auth/token");
            let challenge = format!(
                r#"Bearer realm="{realm}",service="oci-registry",scope="repository:*:*", Basic Realm="oci registry""#,
            );
            response
                .headers_mut()
                .insert("Www-Authenticate", challenge.parse().unwrap());
            response.headers_mut().insert(
                "Docker-Distribution-API-Version",
                "registry/2.0".parse().unwrap(),
            );
        }
        response
    }
}

#[derive(Error, Debug)]
pub enum BusinessError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
}

impl IntoResponse for BusinessError {
    fn into_response(self) -> Response {
        let (status_code, oci_error_info) = match &self {
            Self::Conflict(msg) => (StatusCode::CONFLICT, ErrorInfo::new(ErrorCode::Denied, msg)),
            Self::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorInfo::new(ErrorCode::Unsupported, msg),
            ),
        };
        let body = ErrorResponseBuilder::default()
            .errors(vec![oci_error_info])
            .build()
            .unwrap();
        (status_code, Json(body)).into_response()
    }
}

#[derive(Error, Debug)]
pub enum InternalError {
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Bcrypt error: {0}")]
    Bcrypt(#[from] bcrypt::BcryptError),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Axum error: {0}")]
    Axum(#[from] axum::Error),

    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("Other error: {0}")]
    Others(String),
}

impl IntoResponse for InternalError {
    fn into_response(self) -> Response {
        let error_info =
            ErrorInfo::new(ErrorCode::Unsupported, "an internal server error occurred");
        let body = ErrorResponseBuilder::default()
            .errors(vec![error_info])
            .build()
            .unwrap();
        (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
    }
}

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum HeaderError {
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
}

impl IntoResponse for HeaderError {
    fn into_response(self) -> Response {
        let oci_error = match self {
            Self::ContentRangeMissing => {
                OciError::BlobUploadInvalid("Content-Range header is required".to_string())
            }
            Self::ContentRangeInvalid(reason) => {
                OciError::BlobUploadInvalid(format!("Content-Range header is invalid: {reason}"))
            }
            Self::RangeNotSatisfiable {
                session_id,
                name,
                current_size,
            } => {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(LOCATION, format!("/v2/{name}/blobs/uploads/{session_id}"))
                    .header(RANGE, format!("0-{}", current_size.saturating_sub(1)))
                    .header("Docker-Upload-UUID", session_id)
                    .body(Body::empty())
                    .unwrap();
            }
        };
        oci_error.into_response()
    }
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error(transparent)]
    Oci(#[from] OciError),

    #[error(transparent)]
    Business(#[from] BusinessError),

    #[error(transparent)]
    Internal(#[from] InternalError),

    #[error(transparent)]
    Header(#[from] HeaderError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            Self::Internal(_) => tracing::error!("Internal Server Error: {:?}", self),
            _ => tracing::debug!("Generating response from AppError: {:?}", self),
        }
        match self {
            Self::Oci(e) => e.into_response(),
            Self::Business(e) => e.into_response(),
            Self::Internal(e) => e.into_response(),
            Self::Header(e) => e.into_response(),
        }
    }
}

impl ErrorInfoExt for ErrorInfo {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        ErrorInfoBuilder::default()
            .code(code)
            .message(message.into())
            .detail(serde_json::to_string_pretty(&json!({})).unwrap()) // Start with an empty JSON object
            .build()
            .unwrap()
    }

    fn with_detail(self, detail: serde_json::Value) -> Self {
        ErrorInfoBuilder::default()
            .code(self.code().clone())
            .message(self.message().clone().unwrap())
            .detail(serde_json::to_string_pretty(&detail).unwrap())
            .build()
            .unwrap()
    }
}

pub trait ErrorInfoExt {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self;
    fn with_detail(self, detail: serde_json::Value) -> Self;
}

#[allow(dead_code)]
pub trait MapToAppError<T> {
    fn map_to_oci(self, oci_error: OciError) -> Result<T, AppError>;

    fn map_to_business(self, business_error: BusinessError) -> Result<T, AppError>;

    fn map_to_internal(self) -> Result<T, AppError>;
}

impl<T, E> MapToAppError<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
    InternalError: From<E>,
{
    fn map_to_oci(self, oci_error: OciError) -> Result<T, AppError> {
        self.map_err(|_| AppError::Oci(oci_error))
    }

    fn map_to_business(self, business_error: BusinessError) -> Result<T, AppError> {
        self.map_err(|_| AppError::Business(business_error))
    }

    fn map_to_internal(self) -> Result<T, AppError> {
        self.map_err(|e| AppError::Internal(e.into()))
    }
}
