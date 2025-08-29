use crate::config::Config;
use crate::domain::repo_model::Repo;
use crate::error::{AppError, OciError};
use crate::utils::jwt::{decode, Claims};
use crate::utils::state::AppState;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use std::sync::Arc;
use crate::api::RepoIdentifier;

pub async fn authenticate(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AppError> {
    let claims = extract_claims(req.headers(), state.config.clone());
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
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AppError> {
    let identifier = extract_repo_identifier(req.uri().path());
    if identifier.is_none() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    let identifier = identifier.unwrap();
    let namespace = identifier
        .split("/")
        .find(|s| !s.is_empty())
        .unwrap_or(&identifier);

    let claims = req
        .extensions()
        .get::<Claims>();
    match *req.method() {
        // for read, we can read other's public repos.
        Method::GET | Method::HEAD => {
            println!("identifier: {identifier}");
            if let Ok(repo) = state.repo_storage
                .query_repo_by_name(&identifier)
                .await {
                if repo.is_public == 0 {
                    match claims {
                        Some(claims) => if claims.sub != namespace {
                            return Err(OciError::Forbidden("unable to read others' private repositories".to_string()).into());
                        }
                        None => return Err(OciError::Unauthorized(
                            "unauthorized".to_string(),
                            Some(state.config.clone())
                        ).into())
                    }
                }
            }
        }
        // for write, we cannot write others' all repos.
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE => {
            let claims = claims.unwrap();
            if namespace != claims.sub {
                return Err(OciError::Forbidden("unable to write others' repositories".to_string()).into());
            }
        }
        _ => unreachable!(),
    }
    req.extensions_mut().insert(RepoIdentifier(identifier));
    Ok(next.run(req).await)
}

pub fn extract_claims(headers: &HeaderMap, config: Arc<Config>) -> Result<Claims, AppError> {
    let config_cloned = config.clone();
    let token = headers
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .ok_or_else(|| OciError::Unauthorized("Missing or malformed Bearer token".to_string(), Some(config_cloned)))
        .map(str::to_string)?;
    decode(&config, &token)
}

fn extract_repo_identifier(url: &str) -> Option<String> {
    let segments: Vec<&str> = url
        .split("/")
        .filter(|s| !s.is_empty())
        .collect();
    println!("{:?}", segments);
    match segments.as_slice() {
        // tail: /{name}/manifests/{reference}
        [name @ .., "manifests", _reference] if !name.is_empty() => {
            Some(name.join("/"))
        }
        // tail: /{name}/blobs/{digest}
        [name @ .., "blobs", digest] if !name.is_empty() && *digest != "uploads" => {
            Some(name.join("/"))
        }
        // tail: /{name}/blobs/uploads/
        [name @ .., "blobs", "uploads"] if !name.is_empty() => {
            Some(name.join("/"))
        }
        // tail: /{name}/blobs/uploads/{session_id}
        [name @ .., "blobs", "uploads", _] if !name.is_empty() => {
            Some(name.join("/"))
        }
        // tail: /{name}/tags/list
        [name @ .., "tags", "list"] if !name.is_empty() => {
            Some(name.join("/"))
        }
        // tail: /{name}/referrers/{digest}
        [name @ .., "referrers", _digest] if !name.is_empty() => {
            Some(name.join("/"))
        }
        // tail: /{name}/visibility
        [name @ .., "visibility"] if !name.is_empty() => {
            Some(name.join("/"))
        }
        _ => None,
    }
}