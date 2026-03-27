use axum::Router;
use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use serde::Deserialize;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, oneshot, watch};

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

#[derive(Debug)]
pub struct CallbackResult {
    pub code: String,
    pub state: String,
}

/// Binds a one-shot HTTP server to `127.0.0.1:<random>`.
///
/// Returns the assigned port, a receiver that will carry the callback
/// query-parameters as soon as the browser redirects back, and a
/// shutdown sender to gracefully stop the server.
pub async fn start() -> anyhow::Result<(u16, oneshot::Receiver<CallbackResult>, watch::Sender<()>)>
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let (tx, rx) = oneshot::channel::<CallbackResult>();
    let tx = Arc::new(Mutex::new(Some(tx)));

    let (shutdown_tx, mut shutdown_rx) = watch::channel(());

    let app = Router::new().route(
        "/callback",
        get({
            let tx = tx.clone();
            move |Query(params): Query<CallbackParams>| {
                let tx = tx.clone();
                async move {
                    if let Some(sender) = tx.lock().await.take() {
                        let _ = sender.send(CallbackResult {
                            code: params.code,
                            state: params.state,
                        });
                    }
                    Html(concat!(
                        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Login</title></head>",
                        "<body style=\"font-family:system-ui,sans-serif;display:flex;justify-content:center;",
                        "align-items:center;min-height:100vh;margin:0\">",
                        "<div style=\"text-align:center\">",
                        "<h2>Authentication Successful</h2>",
                        "<p>You can close this tab and return to the terminal.</p>",
                        "</div></body></html>",
                    ))
                }
            }
        }),
    );

    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            })
            .await;
    });

    Ok((port, rx, shutdown_tx))
}
