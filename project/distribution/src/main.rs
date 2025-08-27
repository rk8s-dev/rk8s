use std::path::Path;
use clap::Parser;
use std::sync::Arc;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::signal;
use utils::cli::Args;
use utils::state::AppState;
use crate::config::Config;

mod api;
mod service;
mod storage;
mod utils;
mod error;
mod domain;
mod config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let config = validate_config(&args).await;

    let pool = SqlitePoolOptions::new()
        .max_connections(12)
        .connect(args.database_url.as_str())
        .await?;
    let state = Arc::new(AppState::new(config, Arc::new(pool)).await?);

    let app = api::create_router(state);

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", args.host, args.port))
        .await?;
    println!("listening on {}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
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

async fn validate_config(args: &Args) -> Config {
    let mut validation_errors = Vec::new();

    let root_dir = Path::new(&args.root);
    match tokio::fs::metadata(root_dir).await {
        Ok(meta) => {
            if meta.is_dir() {
                validation_errors.push(format!(
                    "OCI_REGISTRY_ROOTDIR `{}` exists but is not a directory",
                    args.root,
                ));
            }
        }
        Err(_) => {
            validation_errors.push(format!(
                "OCI_REGISTRY_ROOTDIR `{}` does not exist.",
                args.root,
            ))
        }
    }

    let password_salt = std::env::var("PASSWORD_SALT")
        .unwrap_or_else(|_| {
            eprintln!("WARNING: PASSWORD_SALT is not set. Use default value: `salt`");
            "salt".into()
        });
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| {
            eprintln!("WARNING: JWT_SECRET is not set. Use default value: `secret`");
            "secret".into()
    });
    let jwt_lifetime_secs = std::env::var("JWT_LIFETIME_SECONDS")
        .unwrap_or_else(|_| {
            eprintln!("WARNING: JWT_LIFETIME_SECONDS is not set. Use default value: 3600");
            "3600".into()
        })
        .parse::<i64>()
        .unwrap();

    let db_url = Path::new(&args.database_url);
    if let Some(parent) = db_url.parent() {
        if !parent.exists() {
            validation_errors.push(format!(
                "The directory for the database `{}` does not exist",
                parent.display(),
            ));
        }
    }

    if !validation_errors.is_empty() {
        eprintln!("{}", validation_errors.join("\n"));
        std::process::exit(1);
    }

    Config {
        host: args.host.clone(),
        port: args.port,
        storge_typ: args.storage.clone(),
        root_dir: args.root.clone(),
        registry_url: args.url.clone(),
        db_url: args.database_url.clone(),
        jwt_secret,
        password_salt,
        jwt_lifetime_secs,
    }
}