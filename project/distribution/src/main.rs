use crate::config::Config;
use clap::Parser;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::signal;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::EnvFilter;
use utils::cli::Args;
use utils::state::AppState;

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
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let args = Args::parse();
    let config = validate_config(&args).await;

    let pool = SqlitePoolOptions::new()
        .max_connections(12)
        .connect(args.database_url.as_str())
        .await?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await?;
    let state = Arc::new(AppState::new(config, Arc::new(pool)).await);

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", args.host, args.port))
        .await?;
    println!("listening on {}", listener.local_addr()?);

    let app = api::create_router(state);
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
            if !meta.is_dir() {
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
            eprintln!("WARNING: PASSWORD_SALT is not set. Use default value: `ABCDEFGHIJKLMNOP`");
            "ABCDEFGHIJKLMNOP".into()
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

    let db_url = Path::new(args.database_url
        .strip_prefix("sqlite:")
        .unwrap_or_else(|| &args.database_url));
    if let Some(parent) = db_url.parent() {
        tokio::fs::create_dir_all(parent).await.expect("Failed to create db directory");
    }
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(db_url)
        .await
        .expect("Failed to create db file");

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