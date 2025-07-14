mod api;
mod protocol;
mod server;
use api::xlinestore::XlineStore;
use protocol::config::{Config, load_config};
use server::serve;
use std::env;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = env::var("RKS_CONFIG_PATH")
        .unwrap_or_else(|_| format!("{}/tests/config.yaml", std::env::var("CARGO_MANIFEST_DIR").unwrap()));
    let cfg: Config = load_config(&config_path)?;
    let endpoints: Vec<&str> = cfg.xline_endpoints.iter().map(|s| s.as_str()).collect();
    let xline_store = Arc::new(XlineStore::new(&endpoints).await?);
    println!("[rks] listening on {}", cfg.addr);
    serve(cfg.addr, xline_store).await?;
    Ok(())
}
