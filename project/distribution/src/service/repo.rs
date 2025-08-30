use crate::api::RepoIdentifier;
use crate::error::{AppError, BusinessError};
use crate::utils::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeVisReq {
    visibility: String,
}

pub async fn change_visibility(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Extension(identifier): Extension<RepoIdentifier>,
    Json(body): Json<ChangeVisReq>,
) -> Result<impl IntoResponse, AppError> {
    if !name.ends_with("visibility") {
        return Err(
            BusinessError::BadRequest("path must end with `visibility`".to_string()).into(),
        );
    }
    match body.visibility.as_str() {
        "public" | "private" => {
            state
                .repo_storage
                .change_visibility(&identifier.0, body.visibility == "public")
                .await?;
            Ok(StatusCode::OK)
        }
        _ => Err(
            BusinessError::BadRequest("visibility must be `public` or `private`".to_string())
                .into(),
        ),
    }
}
