pub mod v2;
pub mod middleware;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post, put};
use crate::service::repo::change_visibility;
use crate::service::user::{auth, create_user};
use crate::utils::state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router<()> {
    Router::new()
        .nest("/v2", v2::create_v2_router(state.clone()))
        .nest("/api/v1", user_router())
        .with_state(state)
}

fn user_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/users", post(create_user))
        .route("/{*tail}", put(change_visibility))
        .route("/auth/token", get(auth))
}
