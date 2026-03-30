use crate::node::NodeRegistry;
use crate::vault::Vault;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use common::{RksMessage, log_error};
use log::{info, warn};
use std::sync::Arc;

pub struct AppState {
    vault: Arc<Vault>,
    node_registry: Arc<NodeRegistry>,
}

fn router(state: Arc<AppState>) -> Router<()> {
    Router::new()
        .route("/join_token", get(generate_join_token))
        .route("/registry/refresh", post(refresh_registry_credentials))
        .with_state(state)
}

async fn generate_join_token(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state
        .vault
        .generate_once_token()
        .await
        .unwrap_or_else(|e| e.to_string())
}

/// Read all registry credentials from vault and push to every connected worker.
async fn refresh_registry_credentials(
    State(state): State<Arc<AppState>>,
) -> Result<String, (StatusCode, String)> {
    let creds = state.vault.list_registry_credentials().await.map_err(|e| {
        let msg = format!("Failed to load credentials from vault: {e}");
        warn!("{msg}");
        (StatusCode::INTERNAL_SERVER_ERROR, msg)
    })?;

    let msg = RksMessage::SetRegistryCredentials(creds);
    let sessions = state.node_registry.list_sessions().await;
    let total = sessions.len();
    let mut pushed = 0usize;

    for (node_id, session) in &sessions {
        if let Err(e) = session.tx.try_send(msg.clone()) {
            warn!("failed to push credentials to {node_id}: {e}");
        } else {
            pushed += 1;
        }
    }

    Ok(format!("Pushed credentials to {pushed}/{total} workers"))
}

pub async fn start_internal_server(
    vault: Option<Arc<Vault>>,
    node_registry: Arc<NodeRegistry>,
) -> anyhow::Result<()> {
    let Some(vault) = vault else {
        info!(
            target: "rks::internal_server",
            "internal server disabled: no vault available \
             (run `rks gen certs` to initialize, or enable TLS)",
        );
        return Ok(());
    };

    let state = Arc::new(AppState {
        vault,
        node_registry,
    });

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
