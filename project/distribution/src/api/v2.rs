use crate::service::blob::{
    delete_blob_handler, get_blob_handler, get_blob_status_handler, head_blob_handler,
    patch_blob_handler, post_blob_handler, put_blob_handler,
};
use crate::service::manifest::{
    delete_manifest_handler, get_manifest_handler, get_tag_list_handler, head_manifest_handler,
    put_manifest_handler,
};
use crate::utils::state::AppState;
use axum::Router;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{any, get};
use std::collections::HashMap;
use std::sync::Arc;

pub fn create_v2_router() -> Router<Arc<AppState>> {
    // NOTE: dispatch_handler is responsible for handling requests with a `name` containing a `/`
    Router::new()
        // Determine support
        .route("/", get(|| async { StatusCode::OK.into_response() }))
        // List tags
        .route("/{*tail}", any(dispatch_handler))
}

async fn dispatch_handler(
    State(state): State<Arc<AppState>>,
    Path(tail): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
    request: Request,
) -> impl IntoResponse {
    let method = request.method();
    let segments: Vec<&str> = tail.split('/').collect();

    match segments.as_slice() {
        // tail: /{name}/manifests/{reference}
        [name @ .., "manifests", reference] if !name.is_empty() => {
            let name = name.join("/");
            match method {
                // Pull manifests
                &Method::GET => {
                    get_manifest_handler(State(state), Path((name, reference.to_string()))).await
                }
                // Check if manifest exists in the registry
                &Method::HEAD => {
                    head_manifest_handler(State(state), Path((name, reference.to_string()))).await
                }
                // Push Manifests
                &Method::PUT => {
                    put_manifest_handler(State(state), Path((name, reference.to_string())), request)
                        .await
                }
                // Delete manifests or tags
                &Method::DELETE => {
                    delete_manifest_handler(State(state), Path((name, reference.to_string()))).await
                }
                // Unsupported methods
                _ => (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response(),
            }
        }
        // tail: /{name}/blobs/{digest}
        [name @ .., "blobs", digest] if !name.is_empty() && *digest != "uploads" => {
            let name = name.join("/");
            match method {
                // Pull blobs
                &Method::GET => {
                    get_blob_handler(State(state), Path((name, digest.to_string()))).await
                }
                // Check if blob exists in the registry
                &Method::HEAD => {
                    head_blob_handler(State(state), Path((name, digest.to_string()))).await
                }
                // Delete blobs
                &Method::DELETE => {
                    delete_blob_handler(State(state), Path((name, digest.to_string()))).await
                }
                // Unsupported methods
                _ => (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response(),
            }
        }
        // tail: /{name}/blobs/uploads/
        [name @ .., "blobs", "uploads", session_id] if !name.is_empty() && session_id.is_empty() => {
            let name = name.join("/");
            if *method == Method::POST {
                // Open a blob upload session
                post_blob_handler(State(state), Path(name), Query(params), headers, request).await
            } else {
                (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response()
            }
        }
        // tail: /{name}/blobs/uploads/{session_id}
        [name @ .., "blobs", "uploads", session_id] if !name.is_empty() && !session_id.is_empty() => {
            let name = name.join("/");
            match method {
                // Push a blob in chunks
                &Method::PATCH => {
                    patch_blob_handler(
                        State(state),
                        Path((name, session_id.to_string())),
                        headers,
                        request,
                    )
                    .await
                }
                // Close a blob upload session
                &Method::PUT => {
                    put_blob_handler(
                        State(state),
                        Path((name, session_id.to_string())),
                        Query(params),
                        headers,
                        request,
                    )
                    .await
                }
                // Get the status of a blob upload session
                &Method::GET => {
                    get_blob_status_handler(State(state), Path((name, session_id.to_string())))
                        .await
                }
                // Unsupported methods
                _ => (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response(),
            }
        }
        // tail: /{name}/tags/list
        [name @ .., "tags", "list"] if !name.is_empty() => {
            let name = name.join("/");
            if *method == Method::GET {
                // List tags
                get_tag_list_handler(State(state), Path(name), Query(params)).await
            } else {
                (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response()
            }
        }
        _ => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
