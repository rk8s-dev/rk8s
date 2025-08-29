use crate::api::RepoIdentifier;
use crate::error::AppError;
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
    println!("{name}");
    if !name.ends_with("visibility") {
        return Ok((StatusCode::BAD_REQUEST, "path must end with `visibility`").into_response())
    }
    Ok(match body.visibility.as_str() {
        "public" | "private" => {
            state.repo_storage.change_visibility(&identifier.0, body.visibility == "public").await?;
            (
                StatusCode::OK,
            ).into_response()
        }
        _ => (
            StatusCode::BAD_REQUEST,
            "`visibility` must be private or public"
        ).into_response()
    })
}