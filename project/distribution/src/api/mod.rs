pub mod v2;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};
use crate::service::user::{auth, create_user};
use crate::utils::state::AppState;

pub fn create_router() -> Router<Arc<AppState>> {
    Router::new()
        .nest("/v2", v2::create_v2_router())
        .nest("/api/v1", user_router())
}

fn user_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/users", post(create_user))
        // .route("/:namespace/:repo/visibility", put())
        .route("/auth/token", get(auth))
}
