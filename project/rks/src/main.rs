mod api;
mod cli;
mod commands;
mod dns;
mod network;
mod node;
mod protocol;
mod scheduler;
mod vault;

use crate::network::init;
use crate::node::{NodeRegistry, RksNode, Shared};
use crate::protocol::config::load_config;
use crate::{api::xlinestore::XlineStore, scheduler::Scheduler, vault::Vault};
use anyhow::Context;
use clap::Parser;
use cli::{Cli, Commands};
use libscheduler::plugins::{Plugins, node_resources_fit::ScoringStrategy};
use log::{error, info};
use rustls::crypto::CryptoProvider;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .expect("failed to install default CryptoProvider");

    let cli = Cli::parse();

    env_logger::init();

    info!("server started");

    match &cli.command {
        Commands::Start { config } => {
            let cfg = load_config(config.to_str().unwrap())?;
            let xline_config = cfg.xline_config.clone();
            let endpoints: Vec<&str> = xline_config.endpoints.iter().map(|s| s.as_str()).collect();
            let xline_store = Arc::new(XlineStore::new(&endpoints).await?);
            xline_store
                .insert_network_config(&xline_config.prefix, &cfg.network_config)
                .await?;

            info!("[rks] listening on {}", cfg.addr);

            let sm = match init::new_subnet_manager(xline_config.clone()).await {
                Ok(m) => m,
                Err(e) => {
                    error!("Failed to create subnet manager: {e:?}");
                    return Err(e).context("new_subnet_manager failed");
                }
            };
            let local_manager = Arc::new(sm.clone());

            let scheduler = Scheduler::try_new(
                &endpoints,
                xline_store.clone(),
                ScoringStrategy::LeastAllocated,
                Plugins::default(),
            )
            .await
            .context("Failed to create Scheduler")?;

            scheduler.run().await;

            let vault = Vault::migrate().await?;

            let shared = Arc::new(Shared::new(
                xline_store.clone(),
                local_manager.clone(),
                Arc::new(vault),
                Arc::new(NodeRegistry::default()),
            ));

            RksNode::new(cfg.addr.clone(), shared).run().await?;
        }
        Commands::Gen { sub } => sub.handle().await?,
    }

    Ok(())
}
