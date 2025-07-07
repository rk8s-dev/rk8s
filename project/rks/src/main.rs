mod api;
mod protocol;
mod server;
use api::xlinestore::XlineStore;
use protocol::config::{Config, load_config};
use server::serve;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg: Config = load_config("/home/ich/rk8s/project/rks/tests/config.yaml")?;
    let endpoints: Vec<&str> = cfg.xline_endpoints.iter().map(|s| s.as_str()).collect();
    let xline_store = Arc::new(XlineStore::new(&endpoints).await?);
    println!("[rks] listening on {}", cfg.addr);
    serve(cfg.addr, xline_store).await?;
    Ok(())
}
