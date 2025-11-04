use crate::vault::Vault;
use axum::Router;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use common::log_error;
use log::info;
use std::sync::Arc;

pub struct AppState {
    vault: Arc<Vault>,
}

fn router(state: Arc<AppState>) -> Router<()> {
    Router::new()
        .route("/join_token", get(generate_join_token))
        .with_state(state)
}

async fn generate_join_token(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state
        .vault
        .generate_once_token()
        .await
        .unwrap_or_else(|e| e.to_string())
}

pub async fn start_internal_server(vault: Option<Arc<Vault>>) -> anyhow::Result<()> {
    let Some(vault) = vault else {
        info!(
            target: "rks::internal_server",
            "internal server disabled because TLS authentication is disabled",
        );
        return Ok(());
    };

    let state = Arc::new(AppState { vault });

    let router = router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:6789").await?;
    tokio::spawn(async move {
        log_error!(axum::serve(listener, router).await);
    });

    info!(
        target: "rks::internal_server",
        "internal server is listening on 127.0.0.1:6789",
    );
    Ok(())
}
