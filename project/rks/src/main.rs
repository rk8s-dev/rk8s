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

use crate::dns::authority::{run_dns_server, setup_iptable};
use crate::network::init;
use crate::network::manager::LocalManager;
use crate::node::{NodeRegistry, RksNode, Shared};
use crate::protocol::config::{Config, config_ref, load_config};
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
            handle_start_command().await?;
        }
        Commands::Gen { sub } => sub.handle().await?,
    }

    Ok(())
}

async fn handle_start_command() -> anyhow::Result<()> {
    let cfg = config_ref();
    let (xline_options, vault) = prepare_xline_options(cfg).await?;
    let xline_store = init_xline_store(cfg, &xline_options).await?;

    spawn_dns_server(xline_store.clone(), cfg);
    setup_dns_firewall(cfg).await?;

    info!(target: "rks::main", "listening on {}", cfg.addr);

    let local_manager = init_local_manager(cfg, &xline_options).await?;
    launch_scheduler(xline_options, xline_store.clone()).await?;

    let shared = Arc::new(Shared::new(
        xline_store.clone(),
        local_manager,
        vault.clone(),
        Arc::new(NodeRegistry::default()),
    ));

    internal::start_internal_server(vault.clone()).await?;
    RksNode::new(cfg.addr.clone(), shared).run().await
}

async fn prepare_xline_options(cfg: &Config) -> anyhow::Result<(XlineOptions, Option<Arc<Vault>>)> {
    let mut option = XlineOptions::new(cfg.xline_config.endpoints.clone());

    if !cfg.tls_config.enable {
        return Ok((option, None));
    }

    let folder = &cfg.tls_config.vault_folder;
    let root_cert = tokio::fs::read_to_string(folder.join("root.pem")).await?;
    let vault = Arc::new(Vault::migrate().await?);
    let resp = vault.issue_rks_cert().await?;
    option = option.with_tls(&root_cert, &resp.certificate, &resp.private_key)?;

    Ok((option, Some(vault)))
}

async fn init_xline_store(cfg: &Config, option: &XlineOptions) -> anyhow::Result<Arc<XlineStore>> {
    let store = Arc::new(
        XlineStore::new(option.clone())
            .await
            .with_context(|| "Failed to connect xline")?,
    );

    store
        .insert_network_config(&cfg.xline_config.prefix, &cfg.network_config)
        .await?;

    Ok(store)
}

fn spawn_dns_server(xline_store: Arc<XlineStore>, cfg: &Config) {
    info!(target: "rks::main", "initializing dns server");
    let port = cfg.dns_config.port;
    tokio::spawn(async move {
        if let Err(err) = run_dns_server(xline_store, port).await {
            error!(
                target: "rks::main",
                "dns server exited with error: {err:?}"
            );
        }
    });
}

async fn setup_dns_firewall(cfg: &Config) -> anyhow::Result<()> {
    let server_ip = cfg
        .addr
        .split(':')
        .next()
        .unwrap_or("127.0.0.1")
        .to_string();
    setup_iptable(server_ip, cfg.dns_config.port).await
}

async fn init_local_manager(
    cfg: &Config,
    option: &XlineOptions,
) -> anyhow::Result<Arc<LocalManager>> {
    let manager = init::new_subnet_manager(cfg.xline_config.clone(), option.clone())
        .await
        .map_err(|e| {
            error!("Failed to create subnet manager: {e:?}");
            e
        })
        .context("new_subnet_manager failed")?;
    Ok(Arc::new(manager))
}

async fn launch_scheduler(
    option: XlineOptions,
    xline_store: Arc<XlineStore>,
) -> anyhow::Result<()> {
    let scheduler = Scheduler::try_new(
        option,
        xline_store,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
    )
    .await
    .context("Failed to create Scheduler")?;

    scheduler.run().await;
    Ok(())
}
