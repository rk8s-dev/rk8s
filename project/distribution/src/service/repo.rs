use std::sync::Arc;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::Json;
use axum::response::IntoResponse;
use regex::Regex;
use serde::Deserialize;
use crate::error::AppError;
use crate::utils::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeVisReq {
    visibility: i32,
}

pub async fn change_visibility(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<ChangeVisReq>,
) -> Result<impl IntoResponse, AppError> {
    println!("{name}");
    if !name.ends_with("visibility") {
        return Ok((StatusCode::BAD_REQUEST, "path must end with `visibility`").into_response())
    }
    let regex = Regex::new(r#"^([a-zA-Z0-9_-]+(?:/[a-zA-Z0-9_-]+)*)/visibility$"#).unwrap();
    let repo_name = regex.captures(&name).unwrap().get(1).unwrap().as_str();
    Ok(match req.visibility {
        0 | 1 => {
            state.repo_storage.change_visibility(repo_name, req.visibility == 1).await?;
            (
                StatusCode::OK,
            ).into_response()
        }
        _ => (
            StatusCode::BAD_REQUEST,
            "`visibility` must be 0 (private) or 1 (public)`"
        ).into_response()
    })
}