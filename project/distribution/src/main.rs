use clap::Parser;
use std::sync::Arc;
use tokio::signal;
use utils::cli::Args;
use utils::state::AppState;

mod api;
mod service;
mod storage;
mod utils;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let state = Arc::new(AppState::new(&args.storage, &args.root, &args.url));

    let app = api::create_router().with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", args.host, args.port))
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("Shutting down...");
}
