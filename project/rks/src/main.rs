mod api;
mod cli;
mod commands;
mod controllers;
mod csi;
mod dns;
mod internal;
mod network;
mod node;
mod protocol;
mod scheduler;
mod vault;

use crate::controllers::endpoint_controller::EndpointController;
use crate::controllers::garbage_collector::GarbageCollector;
use crate::controllers::{
    CONTROLLER_MANAGER, ControllerManager, DeploymentController, NftablesController,
    ReplicaSetController,
};
use crate::dns::authority::{run_dns_server, setup_dns_nftable};
use crate::network::init;
use crate::network::manager::LocalManager;
use crate::network::service_ip::{ServiceIpAllocator, ServiceIpRegistry};
use crate::node::{NodeRegistry, RksNode, Shared};
use crate::protocol::config::{Config, config_ref, load_config};
use crate::{api::xlinestore::XlineStore, scheduler::Scheduler, vault::Vault};
use anyhow::Context;
use clap::Parser;
use cli::{Cli, Commands};
use ipnetwork::Ipv4Network;
use libscheduler::plugins::{Plugins, node_resources_fit::ScoringStrategy};
use libvault::storage::xline::XlineOptions;
use log::{LevelFilter, error, info};
use rustls::crypto::CryptoProvider;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .expect("failed to install default CryptoProvider");

    let cli = Cli::parse();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("netlink_packet_route", LevelFilter::Error)
        .init();

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
    launch_scheduler(xline_options.clone(), xline_store.clone()).await?;

    let node_registry = Arc::new(NodeRegistry::default());

    let volume_store = Arc::new(csi::VolumeStore::new(xline_store.clone()));
    let csi_controller = Arc::new(csi::RksCsiController::new(volume_store));

    let volume_orchestrator = Arc::new(csi::VolumeOrchestrator::new(
        csi_controller,
        node_registry.clone(),
    ));

    // Get network config and initialize Service IP allocator
    let (network_config, service_ip_allocator) =
        init_service_ip_components(cfg, &local_manager, &xline_options, &xline_store).await?;

    register_controllers(
        CONTROLLER_MANAGER.clone(),
        xline_store.clone(),
        node_registry.clone(),
        4,
    )
    .await?;
    CONTROLLER_MANAGER
        .clone()
        .start_watch(xline_store.clone())
        .await?;

    let shared = Arc::new(Shared::new(
        xline_store.clone(),
        local_manager,
        vault.clone(),
        node_registry,
        network_config,
        service_ip_allocator,
        volume_orchestrator,
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
    setup_dns_nftable(server_ip, cfg.dns_config.port).await
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

async fn init_service_ip_components(
    cfg: &Config,
    local_manager: &Arc<LocalManager>,
    xline_options: &XlineOptions,
    xline_store: &Arc<XlineStore>,
) -> anyhow::Result<(
    Arc<libnetwork::config::NetworkConfig>,
    Option<Arc<ServiceIpAllocator>>,
)> {
    let network_config = Arc::new(local_manager.get_network_config().await?);

    let service_ip_allocator = if let Some(service_cidr) = network_config.service_cidr {
        let registry = Arc::new(
            ServiceIpRegistry::new(cfg.xline_config.clone(), xline_options.clone()).await?,
        );

        rebuild_service_ip_registry_from_services(&registry, xline_store, service_cidr).await?;

        let allocator = ServiceIpAllocator::new(
            service_cidr,
            registry,
            10, // max retries
        );

        info!(
            target: "rks::main",
            "Service IP allocator initialized with CIDR: {}",
            service_cidr
        );

        Some(Arc::new(allocator))
    } else {
        info!(
            target: "rks::main",
            "Service IP allocator not initialized (no service_cidr configured)"
        );
        None
    };

    Ok((network_config, service_ip_allocator))
}

async fn rebuild_service_ip_registry_from_services(
    registry: &ServiceIpRegistry,
    xline_store: &Arc<XlineStore>,
    service_cidr: Ipv4Network,
) -> anyhow::Result<()> {
    let existing_records = registry
        .list_all()
        .await
        .context("failed to list existing service-ip records")?;

    let mut current_by_ip = HashMap::with_capacity(existing_records.len());
    for record in existing_records {
        current_by_ip.insert(record.ip, (record.service_namespace, record.service_name));
    }

    let services = xline_store
        .list_services()
        .await
        .context("failed to list services for service-ip rebuild")?;

    let mut desired_by_ip: HashMap<std::net::Ipv4Addr, (String, String)> = HashMap::new();
    for svc in services {
        let Some(cluster_ip) = svc.spec.cluster_ip.as_ref() else {
            continue;
        };
        let cluster_ip = cluster_ip.trim();
        if cluster_ip.is_empty() || cluster_ip.eq_ignore_ascii_case("none") {
            continue;
        }

        let Ok(ip) = cluster_ip.parse::<std::net::Ipv4Addr>() else {
            error!(
                target: "rks::main",
                "skip invalid Service clusterIP '{}' for {}/{} during rebuild",
                cluster_ip,
                svc.metadata.namespace,
                svc.metadata.name
            );
            continue;
        };

        if !service_cidr.contains(ip) {
            error!(
                target: "rks::main",
                "skip out-of-range Service clusterIP {} for {}/{} during rebuild (serviceCIDR={})",
                ip,
                svc.metadata.namespace,
                svc.metadata.name,
                service_cidr
            );
            continue;
        }

        let owner = (svc.metadata.namespace.clone(), svc.metadata.name.clone());
        if let Some(existing_owner) = desired_by_ip.get(&ip)
            && existing_owner != &owner
        {
            error!(
                target: "rks::main",
                "detected duplicated Service clusterIP {} between {}/{} and {}/{} during rebuild; keeping first owner",
                ip,
                existing_owner.0,
                existing_owner.1,
                owner.0,
                owner.1
            );
            continue;
        }

        desired_by_ip.insert(ip, owner);
    }

    let desired_ips: HashSet<std::net::Ipv4Addr> = desired_by_ip.keys().copied().collect();

    for ip in current_by_ip.keys() {
        if !desired_ips.contains(ip)
            && let Err(e) = registry.release(*ip).await
        {
            error!(
                target: "rks::main",
                "failed to release stale service-ip record {} during rebuild: {}",
                ip,
                e
            );
        }
    }

    for (ip, owner) in desired_by_ip {
        match current_by_ip.get(&ip) {
            Some(current_owner) if current_owner == &owner => {
                continue;
            }
            Some(current_owner) => {
                if let Err(e) = registry.release(ip).await {
                    error!(
                        target: "rks::main",
                        "failed to release conflicted service-ip record {} (current owner {}/{}) during rebuild: {}",
                        ip,
                        current_owner.0,
                        current_owner.1,
                        e
                    );
                    continue;
                }
            }
            None => {}
        }

        if let Err(e) = registry
            .allocate(ip, owner.0.clone(), owner.1.clone())
            .await
        {
            error!(
                target: "rks::main",
                "failed to rebuild service-ip record {} for {}/{}: {}",
                ip,
                owner.0,
                owner.1,
                e
            );
        }
    }

    info!(
        target: "rks::main",
        "service-ip registry rebuild completed from Service objects"
    );
    Ok(())
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

async fn register_controllers(
    mgr: Arc<ControllerManager>,
    xline_store: Arc<XlineStore>,
    node_registry: Arc<NodeRegistry>,
    workers: usize,
) -> anyhow::Result<()> {
    let gc = GarbageCollector::new(xline_store.clone());
    let rs = ReplicaSetController::new(xline_store.clone());
    let ep = EndpointController::new(xline_store.clone());
    let deploy = DeploymentController::new(xline_store.clone());
    let nft = NftablesController::new(xline_store.clone(), node_registry);

    mgr.clone()
        .register(Arc::new(RwLock::new(gc)), workers)
        .await?;
    mgr.clone()
        .register(Arc::new(RwLock::new(rs)), workers)
        .await?;
    mgr.clone()
        .register(Arc::new(RwLock::new(ep)), workers)
        .await?;
    mgr.clone()
        .register(Arc::new(RwLock::new(deploy)), workers)
        .await?;
    mgr.clone()
        .register(Arc::new(RwLock::new(nft)), workers)
        .await?;
    Ok(())
}
