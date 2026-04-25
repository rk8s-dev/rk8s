use crate::domain::repo::{Repo, RepoTag};
use crate::error::{AppError, BusinessError};
use crate::utils::jwt::Claims;
use crate::utils::repo_identifier::RepoIdentifier;
use crate::utils::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

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
    tags: Vec<String>,
    size_tag: Option<String>,
    size_bytes: Option<u64>,
    last_pushed_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Debug)]
struct ListRepoResponse {
    data: Vec<RepoView>,
}

impl RepoView {
    fn from_repo_and_tags(value: Repo, repo_tags: Vec<RepoTag>) -> Self {
        let size_source = repo_tags
            .iter()
            .find(|tag| tag.tag == "latest")
            .or_else(|| repo_tags.first());

        let size_tag = size_source.map(|tag| tag.tag.clone());
        let size_bytes = size_source.and_then(|tag| u64::try_from(tag.manifest_size_bytes).ok());

        Self {
            namespace: value.namespace,
            name: value.name,
            is_public: value.is_public,
            tags: repo_tags.into_iter().map(|tag| tag.tag).collect(),
            size_tag,
            size_bytes,
            last_pushed_at: value.last_pushed_at,
        }
    }
}

fn sort_repo_tags(tags: &mut [RepoTag]) {
    tags.sort_by(
        |left, right| match (left.tag == "latest", right.tag == "latest") {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => right
                .pushed_at
                .cmp(&left.pushed_at)
                .then_with(|| left.tag.cmp(&right.tag)),
        },
    );
}

pub async fn list_visible_repos(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
) -> Result<impl IntoResponse, AppError> {
    let namespace = claims.sub;
    let repos = state
        .repo_storage
        .query_all_visible_repos(&namespace)
        .await?;
    let repo_ids = repos.iter().map(|repo| repo.id).collect::<Vec<Uuid>>();
    let mut tags_by_repo = state
        .repo_storage
        .query_repo_tags(&repo_ids)
        .await?
        .into_iter()
        .fold(HashMap::<Uuid, Vec<RepoTag>>::new(), |mut acc, tag| {
            acc.entry(tag.repo_id).or_default().push(tag);
            acc
        });

    let data = repos
        .into_iter()
        .map(|repo| {
            let mut repo_tags = tags_by_repo.remove(&repo.id).unwrap_or_default();
            sort_repo_tags(&mut repo_tags);
            RepoView::from_repo_and_tags(repo, repo_tags)
        })
        .collect::<Vec<_>>();

    Ok(Json(ListRepoResponse { data }))
}
