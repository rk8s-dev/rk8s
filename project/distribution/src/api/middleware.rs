use crate::api::{AuthHeader, RepoIdentifier, extract_claims};
use crate::error::{AppError, OciError};
use crate::service::check_password;
use crate::utils::jwt::{Claims, decode};
use crate::utils::state::AppState;
use axum::extract::{OptionalFromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum_extra::TypedHeader;
use axum_extra::headers::Authorization;
use axum_extra::headers::authorization::{Basic, Bearer};
use std::sync::Arc;

pub async fn authenticate(
    State(state): State<Arc<AppState>>,
    auth: Option<AuthHeader>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AppError> {
    let claims = extract_claims(auth, &state).await;
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

#[tracing::instrument(skip_all)]
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

    let claims = req.extensions().get::<Claims>();
    match *req.method() {
        // for read, we can read other's public repos.
        Method::GET | Method::HEAD => {
            if let Ok(repo) = state.repo_storage.query_repo_by_name(&identifier).await
                && repo.is_public == 0
            {
                match claims {
                    Some(claims) => {
                        if claims.sub != namespace {
                            return Err(OciError::Forbidden(
                                "unable to read others' private repositories".to_string(),
                            )
                            .into());
                        }
                    }
                    None => {
                        return Err(OciError::Unauthorized(
                            "unauthorized".to_string(),
                            Some(state.config.clone()),
                        )
                        .into());
                    }
                }
            }
        }
        // for write, we cannot write others' all repos.
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE => match claims {
            Some(claims) if claims.sub == "anonymous" => {
                return Err(OciError::Unauthorized(
                    "unauthorized".to_string(),
                    Some(state.config.clone()),
                )
                .into());
            }
            Some(claims) => {
                if namespace != claims.sub {
                    return Err(OciError::Forbidden(
                        "unable to write others' repositories".to_string(),
                    )
                    .into());
                }
            }
            None => {
                return Err(OciError::Unauthorized(
                    "unauthorized".to_string(),
                    Some(state.config.clone()),
                )
                .into());
            }
        },
        _ => unreachable!(),
    }
    req.extensions_mut().insert(RepoIdentifier(identifier));
    Ok(next.run(req).await)
}

fn extract_repo_identifier(url: &str) -> Option<String> {
    let segments: Vec<&str> = url.split("/").filter(|s| !s.is_empty()).collect();
    match segments.as_slice() {
        // tail: /{name}/manifests/{reference}
        [name @ .., "manifests", _reference] if !name.is_empty() => Some(name.join("/")),
        // tail: /{name}/blobs/{digest}
        [name @ .., "blobs", digest] if !name.is_empty() && *digest != "uploads" => {
            Some(name.join("/"))
        }
        // tail: /{name}/blobs/uploads/
        [name @ .., "blobs", "uploads"] if !name.is_empty() => Some(name.join("/")),
        // tail: /{name}/blobs/uploads/{session_id}
        [name @ .., "blobs", "uploads", _] if !name.is_empty() => Some(name.join("/")),
        // tail: /{name}/tags/list
        [name @ .., "tags", "list"] if !name.is_empty() => Some(name.join("/")),
        // tail: /{name}/referrers/{digest}
        [name @ .., "referrers", _digest] if !name.is_empty() => Some(name.join("/")),
        // tail: /{name}/visibility
        [name @ .., "visibility"] if !name.is_empty() => Some(name.join("/")),
        _ => None,
    }
}
