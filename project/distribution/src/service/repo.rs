use crate::api::RepoIdentifier;
use crate::error::{AppError, BusinessError, MapToAppError};
use crate::utils::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde::Deserialize;
use std::sync::Arc;
use chrono::Utc;

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
        return Err(BusinessError::BadRequest("path must end with `visibility`".to_string()).into())
    }
    match body.visibility.as_str() {
        "public" | "private" => {
            state.repo_storage.query_repo_by_name(&identifier.0).await?;
            sqlx::query("UPDATE repos set is_public = $1, updated_at = $2 WHERE name = $3")
                .bind(body.visibility == "public")
                .bind(Utc::now().format("%Y-%m-%d %H:%M:%S").to_string())
                .bind(&identifier.0)
                .execute(state.repo_storage.pool.as_ref())
                .await
                .map_to_internal()?;
            Ok((
                StatusCode::OK,
            ))
        }
        _ => Err(BusinessError::BadRequest("visibility must be `public` or `private`".to_string()).into())
    }
}