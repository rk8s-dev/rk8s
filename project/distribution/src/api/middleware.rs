use std::sync::Arc;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::IntoResponse;
use crate::config::Config;
use crate::domain::repo_model::Repo;
use crate::error::AppError;
use crate::utils::jwt::{decode, Claims};
use crate::utils::state::AppState;

pub async fn resource_exists(
    State(state): State<Arc<AppState>>,
    Path((name, _)): Path<(String, String)>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AppError> {
    let repo = state.repo_storage.query_repo_by_name(&name).await;
    match *req.method() {
        Method::GET | Method::HEAD | Method::DELETE | Method::PATCH => {
            if let Ok(repo) = repo {
                req.extensions_mut().insert(repo);
            }
        }
        Method::POST | Method::PUT => {
            req.extensions_mut().insert(repo?);
        }
        _ => unreachable!(),
    }
    Ok(next.run(req).await)
}

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
    Path((name, _)): Path<(String, String)>,
    req: Request,
    next: Next,
) -> Result<impl IntoResponse, AppError> {
    let claims = req
        .extensions()
        .get::<Claims>();
    let namespace = name
        .split("/")
        .next()
        .unwrap_or(&name);
    match *req.method() {
        // for read, we can read other's public repos.
        Method::GET | Method::HEAD => {
            let repo = req
                .extensions()
                .get::<Repo>()
                .unwrap();
            if repo.is_public != 1 {
                let claims = claims.ok_or(AppError::Unauthorized("not authorized".to_string(), Some(state.config.clone())))?;
                if claims.sub != namespace {
                    return Err(AppError::Forbidden("unable to read others' private repositories".to_string()));
                }
            }
        }
        // for write, we cannot write others' all repos.
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE => {
            let claims = claims.unwrap();
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
