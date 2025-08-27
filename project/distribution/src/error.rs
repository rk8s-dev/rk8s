use std::sync::Arc;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;
use crate::config::Config;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Bcrypt error: {0}")]
    Bcrypt(#[from] bcrypt::BcryptError),

    #[error("jwt error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("{0} not found")]
    NotFound(String),

    #[error("username {0} is already taken")]
    UsernameTaken(String),

    #[error("invalid password")]
    InvalidPassword,

    #[error("{0}")]
    Unauthorized(String, Option<Arc<Config>>),

    #[error("{0}")]
    Forbidden(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            Self::Sqlx(_) | Self::Migration(_) | Self::Bcrypt(_) => {
                tracing::error!("{self}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error".to_string()).into_response()
            }
            Self::InvalidPassword => {
                (StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            Self::UsernameTaken(_) => {
                (StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            Self::NotFound(_) => {
                (StatusCode::NOT_FOUND, self.to_string()).into_response()
            }
            Self::Jwt(_) => {
                (StatusCode::UNAUTHORIZED, self.to_string()).into_response()
            }
            Self::Unauthorized(_, config) => {
                // This is a little tricky.
                if let Some(config) = config {
                    let realm = format!("{}/auth/token", config.registry_url);
                    let challenge = format!(
                        r#"Bearer realm="{realm}",service="oci-registry",scope="repository:*.*""#,
                    );
                    return (
                        StatusCode::UNAUTHORIZED,
                        [
                            ("Www-Authenticate", challenge)
                        ],
                        self.to_string(),
                    ).into_response();
                }
                (StatusCode::UNAUTHORIZED, self.to_string()).into_response()
            }
            Self::Forbidden(_) => {
                (StatusCode::FORBIDDEN, self.to_string()).into_response()
            }
        }
    }
}

