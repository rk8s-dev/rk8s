use crate::domain::repo::Repo;
use crate::error::{AppError, BusinessError};
use crate::utils::jwt::Claims;
use crate::utils::repo_identifier::RepoIdentifier;
use crate::utils::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeVisibilityRequest {
    visibility: String,
}

pub async fn change_visibility(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Extension(identifier): Extension<RepoIdentifier>,
    Json(body): Json<ChangeVisibilityRequest>,
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
                .change_visibility(&identifier, body.visibility == "public")
                .await?;
            Ok(StatusCode::OK)
        }
        _ => Err(
            BusinessError::BadRequest("visibility must be `public` or `private`".to_string())
                .into(),
        ),
    }
}

#[derive(Serialize, Debug)]
struct RepoView {
    namespace: String,
    name: String,
    is_public: bool,
}

impl From<Repo> for RepoView {
    fn from(value: Repo) -> Self {
        Self {
            namespace: value.namespace,
            name: value.name,
            is_public: value.is_public,
        }
    }
}

pub async fn list_visible_repos(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
) -> Result<impl IntoResponse, AppError> {
    let namespace = claims.sub;
    let repos = state
        .repo_storage
        .query_all_visible_repos(&namespace)
        .await?
        .into_iter()
        .map(RepoView::from)
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "data": repos,
    })))
}
