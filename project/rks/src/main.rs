mod api;
mod cli;
mod commands;
mod protocol;
mod server;
use api::xlinestore::XlineStore;
use clap::Parser;
use cli::{Cli, Commands};
use protocol::config::load_config;
use server::serve;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::Start { config } => {
            let cfg = load_config(config.to_str().unwrap())?;
            let endpoints: Vec<&str> = cfg
                .xline_config
                .endpoints
                .iter()
                .map(|s| s.as_str())
                .collect();
            let xline_store = Arc::new(XlineStore::new(&endpoints).await?);
            println!("[rks] listening on {}", cfg.addr);
            serve(cfg.addr, xline_store).await?;
        }
    }

    Ok(())
}
