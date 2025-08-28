use crate::config::Config;
use crate::domain::repo_model::Repo;
use crate::error::AppError;
use crate::utils::jwt::{decode, Claims};
use crate::utils::state::AppState;
use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::IntoResponse;
use std::sync::Arc;

#[tracing::instrument(skip_all)]
pub async fn authenticate(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AppError> {
    let claims = extract_claims(&req, state.config.clone());
    match *req.method() {
        Method::GET | Method::HEAD => {
            if let Ok(claims) = claims {
                req.extensions_mut().insert(claims);
            }
        }
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE => {
            req.extensions_mut().insert(claims?);
        }
        _ => unreachable!(),
    }
    Ok(next.run(req).await)
}

pub async fn authorize(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<impl IntoResponse, AppError> {
    let repo_name = extract_full_repo_name(req.uri().path());
    let claims = req
        .extensions()
        .get::<Claims>();
    let namespace = repo_name
        .split("/")
        .find(|s| !s.is_empty())
        .unwrap_or(&repo_name);
    match *req.method() {
        // for read, we can read other's public repos.
        Method::GET | Method::HEAD => {
            if let Some(repo) = req
                .extensions()
                .get::<Repo>() {
                if repo.is_public != 1 {
                    let claims = claims.ok_or(AppError::Unauthorized("not authorized".to_string(), Some(state.config.clone())))?;
                    if claims.sub != namespace {
                        return Err(AppError::Forbidden("unable to read others' private repositories".to_string()));
                    }
                }
            }
        }
        // for write, we cannot write others' all repos.
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE => {
            let claims = claims.unwrap();
            println!("namespace: {}", namespace);
            if namespace != claims.sub {
                return Err(AppError::Forbidden("unable to write others' repositories".to_string()));
            }
        }
        _ => unreachable!(),
    }
    Ok(next.run(req).await)
}

fn extract_claims(req: &Request, config: Arc<Config>) -> Result<Claims, AppError> {
    let config_cloned = config.clone();
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Unauthorized("Missing or malformed Bearer token".to_string(), Some(config_cloned)))
        .map(str::to_string)?;
    decode(&config, &token)
}

fn extract_full_repo_name(url: &str) -> String {
    let segments: Vec<&str> = url.split("/").collect();
    match segments.as_slice() {
        // tail: /{name}/manifests/{reference}
        [name @ .., "manifests", _reference] if !name.is_empty() => {
            name.join("/")
        }
        // tail: /{name}/blobs/{digest}
        [name @ .., "blobs", digest] if !name.is_empty() && *digest != "uploads" => {
            name.join("/")
        }
        // tail: /{name}/blobs/uploads/
        // tail: /{name}/blobs/uploads/{session_id}
        [name @ .., "blobs", "uploads", _] if !name.is_empty() => {
            name.join("/")
        }
        // tail: /{name}/tags/list
        [name @ .., "tags", "list"] if !name.is_empty() => {
            name.join("/")
        }
        // tail: /{name}/referrers/{digest}
        [name @ .., "referrers", _digest] if !name.is_empty() => {
            name.join("/")
        }
        _ => unreachable!(),
    }
}