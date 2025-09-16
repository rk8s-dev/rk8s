use crate::api::middleware::{authorize_repository_access, populate_oci_claims};
use crate::api::{AuthHeader, extract_claims};
use crate::error::AppError;
use crate::service::blob::{
    delete_blob_handler, get_blob_handler, get_blob_status_handler, head_blob_handler,
    patch_blob_handler, post_blob_handler, put_blob_handler,
};
use crate::service::manifest::{
    delete_manifest_handler, get_manifest_handler, get_tag_list_handler, head_manifest_handler,
    put_manifest_handler,
};
use crate::utils::state::AppState;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Router, middleware};
use std::collections::HashMap;
use std::sync::Arc;

pub fn create_v2_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(probe))
        .route("/{*tail}", any(dispatch_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            authorize_repository_access,
        ))
        .layer(middleware::from_fn_with_state(state, populate_oci_claims))
}

pub async fn probe(
    State(state): State<Arc<AppState>>,
    auth: Option<AuthHeader>,
) -> Result<impl IntoResponse, AppError> {
    match extract_claims(
        auth,
        &state.config.jwt_secret,
        state.user_storage.as_ref(),
        &state.config.registry_url,
    )
    .await
    {
        Ok(_) => Ok((
            StatusCode::OK,
            [("Docker-Distribution-API-Version", "registry/2.0")],
        )
            .into_response()),
        Err(e) => Err(e),
    }
}

// todo: Perhaps we can extract all required path parameters within middlewares to simplify this.
async fn dispatch_handler(
    State(state): State<Arc<AppState>>,
    Path(tail): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    request: Request,
) -> Result<Response, AppError> {
    let method = request.method();
    let segments: Vec<&str> = tail.split('/').collect();

    match segments.as_slice() {
        // tail: /{name}/manifests/{reference}
        [name @ .., "manifests", reference] if !name.is_empty() => {
            let name = name.join("/");
            match *method {
                // Pull manifests
                Method::GET => {
                    get_manifest_handler(State(state), Path((name, reference.to_string())))
                        .await
                        .map(|res| res.into_response())
                }
                // Check if manifest exists in the registry
                Method::HEAD => {
                    head_manifest_handler(State(state), Path((name, reference.to_string())))
                        .await
                        .map(|res| res.into_response())
                }
                // Push Manifests
                Method::PUT => {
                    put_manifest_handler(State(state), Path((name, reference.to_string())), request)
                        .await
                        .map(|res| res.into_response())
                }
                // Delete manifests or tags
                Method::DELETE => {
                    delete_manifest_handler(State(state), Path((name, reference.to_string())))
                        .await
                        .map(|res| res.into_response())
                }
                // Unsupported methods
                _ => Ok((StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response()),
            }
        }
        // tail: /{name}/blobs/{digest}
        [name @ .., "blobs", digest] if !name.is_empty() && *digest != "uploads" => {
            let name = name.join("/");
            match *method {
                // Pull blobs
                Method::GET => get_blob_handler(State(state), Path((name, digest.to_string())))
                    .await
                    .map(|res| res.into_response()),
                // Check if blob exists in the registry
                Method::HEAD => head_blob_handler(State(state), Path((name, digest.to_string())))
                    .await
                    .map(|res| res.into_response()),
                // Delete blobs
                Method::DELETE => {
                    delete_blob_handler(State(state), Path((name, digest.to_string())))
                        .await
                        .map(|res| res.into_response())
                }
                // Unsupported methods
                _ => Ok((StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response()),
            }
        }
        // tail: /{name}/blobs/uploads/
        [name @ .., "blobs", "uploads", session_id]
            if !name.is_empty() && session_id.is_empty() =>
        {
            let name = name.join("/");
            if *method == Method::POST {
                // Open a blob upload session
                post_blob_handler(State(state), Path(name), Query(params), headers, request)
                    .await
                    .map(|res| res.into_response())
            } else {
                Ok((StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response())
            }
        }
        // tail: /{name}/blobs/uploads/{session_id}
        [name @ .., "blobs", "uploads", session_id]
            if !name.is_empty() && !session_id.is_empty() =>
        {
            let name = name.join("/");
            match *method {
                // Push a blob in chunks
                Method::PATCH => patch_blob_handler(
                    State(state),
                    Path((name, session_id.to_string())),
                    headers,
                    request,
                )
                .await
                .map(|res| res.into_response()),
                // Close a blob upload session
                Method::PUT => put_blob_handler(
                    State(state),
                    Path((name, session_id.to_string())),
                    Query(params),
                    request,
                )
                .await
                .map(|res| res.into_response()),
                // Get the status of a blob upload session
                Method::GET => {
                    get_blob_status_handler(State(state), Path((name, session_id.to_string())))
                        .await
                        .map(|res| res.into_response())
                }
                // Unsupported methods
                _ => Ok((StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response()),
            }
        }
        // tail: /{name}/tags/list
        [name @ .., "tags", "list"] if !name.is_empty() => {
            let name = name.join("/");
            if *method == Method::GET {
                // List tags
                get_tag_list_handler(State(state), Path(name), Query(params))
                    .await
                    .map(|res| res.into_response())
            } else {
                Ok((StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response())
            }
        }
        _ => Ok((StatusCode::NOT_FOUND, "not found").into_response()),
    }
}
