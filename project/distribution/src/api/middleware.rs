use crate::api::AuthHeader;
use crate::error::{AppError, InternalError, OciError};
use crate::utils::jwt::Claims;
use crate::utils::repo_identifier::identifier_from_full_name;
use crate::utils::state::AppState;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
struct VerifyRequest {
    token: String,
    namespace: Option<String>,
    repository: Option<String>,
    action: Option<String>,
}

#[derive(Deserialize)]
struct VerifyResponse {
    valid: bool,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    user: Option<VerifyUser>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct VerifyUser {
    username: String,
}

async fn verify_with_web_app(
    state: &Arc<AppState>,
    token: String,
    namespace: Option<String>,
    repository: Option<String>,
    action: Option<String>,
) -> Result<Claims, AppError> {
    let requested_action = action.clone();

    let resp = state
        .http_client
        .post(&state.config.auth_api_url)
        .header(
            "X-Registry-Internal-Token",
            &state.config.internal_verify_token,
        )
        .json(&VerifyRequest {
            token,
            namespace,
            repository,
            action,
        })
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to send verify request: {}", e);
            InternalError::Others("authentication service unavailable".to_string())
        })?;

    let status = resp.status();
    let verify_res: VerifyResponse = resp.json().await.map_err(|e| {
        tracing::error!("Failed to parse verify response: {}", e);
        InternalError::Others("invalid verify response payload".to_string())
    })?;

    let verify_error_msg = verify_res
        .error
        .clone()
        .unwrap_or_else(|| "verify_failed".to_string());

    match status {
        StatusCode::OK => {
            if !verify_res.valid {
                return Err(OciError::Unauthorized {
                    msg: verify_error_msg,
                    auth_url: Some(state.config.registry_url.clone()),
                }
                .into());
            }
        }
        StatusCode::UNAUTHORIZED => {
            if matches!(
                verify_res.error.as_deref(),
                Some("missing_internal_token" | "invalid_internal_token")
            ) {
                return Err(InternalError::Others(
                    "authentication service rejected internal credentials".to_string(),
                )
                .into());
            }
            return Err(OciError::Unauthorized {
                msg: verify_error_msg,
                auth_url: Some(state.config.registry_url.clone()),
            }
            .into());
        }
        StatusCode::FORBIDDEN => {
            return Err(OciError::Forbidden(verify_error_msg).into());
        }
        s if s.is_server_error() => {
            return Err(InternalError::Others(format!(
                "authentication service internal error: {}",
                verify_error_msg
            ))
            .into());
        }
        other => {
            return Err(InternalError::Others(format!(
                "unexpected verify status: {} ({})",
                other, verify_error_msg
            ))
            .into());
        }
    }

    if let Some(action) = requested_action.as_deref()
        && !verify_res.permissions.iter().any(|p| p == action)
    {
        return Err(OciError::Forbidden(format!("permission denied for action: {action}")).into());
    }

    let Some(user) = verify_res.user else {
        return Err(InternalError::Others(
            "verify contract violation: missing user on successful response".to_string(),
        )
        .into());
    };

    if user.username.trim().is_empty() {
        return Err(
            InternalError::Others("verify contract violation: empty username".to_string()).into(),
        );
    }

    Ok(Claims {
        sub: user.username,
        exp: 0, // Not strictly needed for the internal claims extension if verified by web
        iss: Some("libra.tools".to_string()),
        iat: None,
    })
}

fn is_anonymous_subject(subject: &str) -> bool {
    subject.eq_ignore_ascii_case("anonymous")
}

pub async fn require_authentication(
    State(state): State<Arc<AppState>>,
    auth: Option<AuthHeader>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AppError> {
    let token = match auth {
        Some(AuthHeader::Bearer(bearer)) => bearer.token().to_string(),
        _ => {
            return Err(OciError::Unauthorized {
                msg: "Missing or invalid authorization header".to_string(),
                auth_url: Some(state.config.registry_url.clone()),
            }
            .into());
        }
    };

    let claims = verify_with_web_app(&state, token, None, None, None).await?;
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

pub async fn populate_oci_claims(
    State(state): State<Arc<AppState>>,
    auth: Option<AuthHeader>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AppError> {
    let token = match auth {
        Some(AuthHeader::Bearer(bearer)) => Some(bearer.token().to_string()),
        _ => None,
    };

    let claims = if let Some(t) = token {
        Some(verify_with_web_app(&state, t, None, None, None).await)
    } else {
        None
    };

    match *req.method() {
        Method::GET | Method::HEAD => {
            if let Some(Ok(claims)) = claims {
                req.extensions_mut().insert(claims);
            }
        }
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE => {
            let claims = match claims {
                Some(Ok(claims)) => claims,
                Some(Err(err)) => return Err(err),
                None => {
                    return Err(OciError::Unauthorized {
                        msg: "unauthorized".to_string(),
                        auth_url: Some(state.config.registry_url.clone()),
                    }
                    .into());
                }
            };
            req.extensions_mut().insert(claims);
        }
        _ => unreachable!(),
    }
    Ok(next.run(req).await)
}

pub async fn authorize_repository_access(
    State(state): State<Arc<AppState>>,
    auth: Option<AuthHeader>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AppError> {
    let identifier = extract_full_repo_name(req.uri().path());
    if identifier.is_none() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    let identifier = identifier_from_full_name(identifier.unwrap());
    let namespace = &identifier.namespace;
    match *req.method() {
        // for read, we can read others' public repos.
        Method::GET | Method::HEAD => {
            if let Ok(repo) = state
                .repo_storage
                .query_repo_by_identifier(&identifier)
                .await
                && !repo.is_public
            {
                let token = match auth.as_ref() {
                    Some(AuthHeader::Bearer(bearer)) => bearer.token().to_string(),
                    _ => {
                        return Err(OciError::Unauthorized {
                            msg: "unauthorized".to_string(),
                            auth_url: Some(state.config.registry_url.clone()),
                        }
                        .into());
                    }
                };

                let _claims = verify_with_web_app(
                    &state,
                    token,
                    Some(namespace.clone()),
                    Some(identifier.name.clone()),
                    Some("pull".to_string()),
                )
                .await?;
            }
        }
        // for write, we cannot write others' all repos.
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE => {
            let token = match auth.as_ref() {
                Some(AuthHeader::Bearer(bearer)) => bearer.token().to_string(),
                _ => {
                    return Err(OciError::Unauthorized {
                        msg: "unauthorized".to_string(),
                        auth_url: Some(state.config.registry_url.clone()),
                    }
                    .into());
                }
            };

            let _claims = verify_with_web_app(
                &state,
                token,
                Some(namespace.clone()),
                Some(identifier.name.clone()),
                Some("push".to_string()),
            )
            .await?;
        }
        _ => unreachable!(),
    }
    req.extensions_mut().insert(identifier);
    Ok(next.run(req).await)
}

fn extract_full_repo_name(url: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::is_anonymous_subject;

    #[test]
    fn anonymous_subject_match_is_case_insensitive() {
        assert!(is_anonymous_subject("anonymous"));
        assert!(is_anonymous_subject("Anonymous"));
        assert!(!is_anonymous_subject("lingbou"));
    }
}
