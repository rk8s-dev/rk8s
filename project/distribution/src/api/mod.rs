pub mod v2;
pub mod middleware;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post, put};
use crate::api::middleware::{authenticate, authorize};
use crate::api::v2::probe;
use crate::service::repo::change_visibility;
use crate::service::user::{auth, create_user};
use crate::utils::state::AppState;

#[derive(Debug, Clone)]
pub struct RepoIdentifier(pub String);

pub fn create_router(state: Arc<AppState>) -> Router<()> {
    Router::new()
        .route("/v2/", get(probe))
        .nest("/v2", v2::create_v2_router(state.clone()))
        .nest("/api/v1", user_router(state.clone()))
        .route("/auth/token", get(auth))
        .with_state(state)
}

fn user_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/users", post(create_user))
        .route("/{*tail}", put(change_visibility)
            .layer(axum::middleware::from_fn_with_state(state.clone(), authorize))
            .layer(axum::middleware::from_fn_with_state(state, authenticate)))
}
