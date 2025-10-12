mod api;
mod cli;
mod commands;
mod dns;
mod network;
mod protocol;
mod scheduler;
mod server;

use crate::dns::authority::run_dns_server;
use crate::network::init;
use crate::protocol::config::load_config;
use crate::{api::xlinestore::XlineStore, scheduler::Scheduler};
use anyhow::Context;
use clap::Parser;
use cli::{Cli, Commands};
use libscheduler::plugins::{Plugins, node_resources_fit::ScoringStrategy};
use log::error;
use server::serve;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    use log::info;

    env_logger::init();

    info!("server started");

    match &cli.command {
        Commands::Start { config } => {
            let cfg = load_config(config.to_str().unwrap())?;
            let xline_config = cfg.xline_config;
            let endpoints: Vec<&str> = xline_config.endpoints.iter().map(|s| s.as_str()).collect();
            let xline_store = Arc::new(XlineStore::new(&endpoints).await?);
            xline_store
                .insert_network_config(&xline_config.prefix, &cfg.network_config)
                .await?;
            let store = xline_store.clone();
            println!("[rks] init dns server");
            tokio::spawn(async move {
                let _ = run_dns_server(store, cfg.dns_config.port).await;
            });
            let server_ip = cfg
                .addr
                .clone()
                .split(':')
                .next()
                .unwrap_or("127.0.0.1")
                .to_string();
            crate::dns::authority::setup_iptable(server_ip, cfg.dns_config.port).await?;
            println!("[rks] listening on {}", cfg.addr);
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
            serve(cfg.addr, xline_store, local_manager, cfg.dns_config.port).await?;
        }
    }

    Ok(())
}
