mod api;
mod cli;
mod commands;
mod dns;
mod internal;
mod network;
mod node;
mod protocol;
mod scheduler;
mod vault;

use crate::network::init;
use crate::node::{NodeRegistry, RksNode, Shared};
use crate::protocol::config::{config_ref, load_config};
use crate::{api::xlinestore::XlineStore, scheduler::Scheduler, vault::Vault};
use anyhow::Context;
use clap::Parser;
use cli::{Cli, Commands};
use libscheduler::plugins::{Plugins, node_resources_fit::ScoringStrategy};
use libvault::storage::xline::XlineOptions;
use log::{error, info};
use rustls::crypto::CryptoProvider;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .expect("failed to install default CryptoProvider");

    let cli = Cli::parse();

    env_logger::init();

    match &cli.command {
        Commands::Start { config } => {
            load_config(config.to_str().unwrap())?;
            let cfg = config_ref();
            let xline_config = cfg.xline_config.clone();

            let folder = &cfg.tls_config.vault_folder;
            let endpoints = xline_config.endpoints.clone();

            let root_cert = tokio::fs::read_to_string(folder.join("root.pem")).await?;
            let vault = Vault::migrate().await?;

            let mut option = XlineOptions::new(endpoints.clone());
            if cfg.tls_config.enable {
                let resp = vault.issue_rks_cert().await?;
                option = option.with_tls(&root_cert, &resp.certificate, &resp.private_key)?;
            }

            let xline_store = Arc::new(XlineStore::new(option.clone()).await?);
            xline_store
                .insert_network_config(&xline_config.prefix, &cfg.network_config)
                .await?;

            info!("[rks] listening on {}", cfg.addr);

            let sm = match init::new_subnet_manager(xline_config.clone(), option.clone()).await {
                Ok(m) => m,
                Err(e) => {
                    error!("Failed to create subnet manager: {e:?}");
                    return Err(e).context("new_subnet_manager failed");
                }
            };
            let local_manager = Arc::new(sm.clone());

            let scheduler = Scheduler::try_new(
                option,
                xline_store.clone(),
                ScoringStrategy::LeastAllocated,
                Plugins::default(),
            )
            .await
            .context("Failed to create Scheduler")?;

            scheduler.run().await;

            let vault = Arc::new(vault);
            let shared = Arc::new(Shared::new(
                xline_store.clone(),
                local_manager.clone(),
                vault.clone(),
                Arc::new(NodeRegistry::default()),
            ));

            internal::start_internal_server(vault).await?;
            RksNode::new(cfg.addr.clone(), shared).run().await?;
        }
        Commands::Gen { sub } => sub.handle().await?,
    }

    Ok(())
}
