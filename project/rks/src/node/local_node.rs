use crate::network::manager::LocalManager;
use crate::node::register::build_node_network_config;
use crate::node::{Shared, WorkerSession};
use anyhow::{Context, Result};
use common::RksMessage;
use common::lease::LeaseAttrs;
use ipnetwork::{IpNetwork, Ipv4Network};
use libcni::ip::route::{self as ip_route, Route};
use libnetwork::ip::{PublicIPOpts, get_ip_family, lookup_ext_iface};
use libnetwork::route::{RouteListOps, RouteManager};
use log::{debug, error, info, warn};
use netlink_packet_route::AddressFamily;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

const DEFAULT_SUBNET_FILE: &str = "/etc/cni/net.d/subnet.env";
const LOCAL_SESSION_CHANNEL_SIZE: usize = 32;

pub(crate) async fn bootstrap(shared: Arc<Shared>, addr: &str) -> Result<()> {
    let network_config = shared
        .local_manager
        .get_network_config()
        .await
        .context("failed to fetch network config for local rks node")?;

    let ip_stack = get_ip_family(network_config.enable_ipv4, network_config.enable_ipv6)
        .context("failed to determine local rks node IP stack")?;

    let ext_iface = lookup_ext_iface(
        None,
        None,
        None,
        ip_stack,
        PublicIPOpts {
            public_ip: None,
            public_ipv6: None,
        },
    )
    .await
    .context("failed to discover external interface for local rks node")?;

    let node_id = resolve_local_node_id(addr);
    let public_ip = ext_iface
        .ext_addr
        .or(ext_iface.iface_addr)
        .unwrap_or(Ipv4Addr::UNSPECIFIED);
    let public_ipv6 = ext_iface.ext_v6_addr.or(ext_iface.iface_v6_addr);

    let lease_attrs = LeaseAttrs {
        public_ip,
        public_ipv6,
        backend_type: network_config.backend_type.clone(),
        node_id: node_id.clone(),
        ..Default::default()
    };

    let lease = shared
        .local_manager
        .acquire_lease(&lease_attrs)
        .await
        .with_context(|| format!("failed to acquire lease for local rks node {node_id}"))?;

    let (msg_tx, msg_rx) = mpsc::channel::<RksMessage>(LOCAL_SESSION_CHANNEL_SIZE);
    let session = Arc::new(WorkerSession::new(None, msg_tx.clone(), lease.clone()));
    shared
        .node_registry
        .register(node_id.clone(), session.clone())
        .await;

    let route_manager = Arc::new(Mutex::new(RouteManager::new(
        ext_iface.iface.index,
        network_config.backend_type.clone(),
    )));

    spawn_local_session_loop(
        node_id.clone(),
        shared.local_manager.clone(),
        route_manager,
        msg_rx,
    );

    spawn_local_lease_completion(node_id.clone(), shared.local_manager.clone(), session);

    let node_net_config = build_node_network_config(
        node_id.clone(),
        &network_config,
        false,
        Some(lease.subnet),
        lease.ipv6_subnet,
    )
    .with_context(|| format!("failed to build network config payload for {node_id}"))?;

    if let Err(e) = msg_tx.try_send(RksMessage::SetNetwork(Box::new(node_net_config))) {
        warn!("Failed to enqueue local subnet config for {node_id}: {e}");
    }

    info!(
        target: "rks::node::local",
        "local rks node registered: id={}, lease={}, iface_index={}",
        node_id,
        lease.subnet,
        ext_iface.iface.index
    );

    Ok(())
}

fn spawn_local_session_loop(
    node_id: String,
    local_manager: Arc<LocalManager>,
    route_manager: Arc<Mutex<RouteManager>>,
    mut msg_rx: mpsc::Receiver<RksMessage>,
) {
    tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            match msg {
                RksMessage::SetNetwork(config) => {
                    if config.node_id != node_id {
                        debug!(
                            target: "rks::node::local",
                            "ignoring SetNetwork for node {} (self={})",
                            config.node_id,
                            node_id
                        );
                        continue;
                    }

                    if let Err(e) =
                        apply_subnet_env(local_manager.clone(), &config.subnet_env).await
                    {
                        error!(
                            target: "rks::node::local",
                            "failed to apply subnet config for node {}: {e:#}",
                            node_id
                        );
                    }
                }
                RksMessage::UpdateRoutes(target, routes) => {
                    if target != node_id {
                        debug!(
                            target: "rks::node::local",
                            "ignoring UpdateRoutes for node {} (self={})",
                            target,
                            node_id
                        );
                        continue;
                    }

                    apply_routes(route_manager.clone(), routes).await;
                }
                RksMessage::SetNftablesRules(_) | RksMessage::UpdateNftablesRules(_) => {
                    debug!(
                        target: "rks::node::local",
                        "skip nftables payload for local rks node {}",
                        node_id
                    );
                }
                other => {
                    debug!(
                        target: "rks::node::local",
                        "ignoring unsupported local message: {other:?}"
                    );
                }
            }
        }

        warn!(
            target: "rks::node::local",
            "local session channel closed for node {}",
            node_id
        );
    });
}

fn spawn_local_lease_completion(
    node_id: String,
    local_manager: Arc<LocalManager>,
    session: Arc<WorkerSession>,
) {
    tokio::spawn(async move {
        if let Err(e) = local_manager
            .complete_lease(session.lease.clone(), session.cancel_notify.clone())
            .await
        {
            warn!(
                target: "rks::node::local",
                "lease completion loop ended for node {}: {e:#}",
                node_id
            );
        }
    });
}

async fn apply_subnet_env(local_manager: Arc<LocalManager>, subnet_env: &str) -> Result<()> {
    let mut ipv4_subnet = None;
    let mut ip_masq = true;
    let mut mtu = 1500;

    for line in subnet_env.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "RKL_SUBNET" => {
                    ipv4_subnet = Some(value.parse::<Ipv4Network>().with_context(|| {
                        format!("invalid RKL_SUBNET value in local config: {value}")
                    })?);
                }
                "RKL_MTU" => {
                    mtu = value.parse::<u32>().unwrap_or(1500);
                }
                "RKL_IPMASQ" => {
                    ip_masq = value.parse::<bool>().unwrap_or(true);
                }
                _ => {}
            }
        }
    }

    let subnet = ipv4_subnet.context("missing RKL_SUBNET in local network config payload")?;
    let config = local_manager
        .get_network_config()
        .await
        .context("failed to reload network config when applying local subnet config")?;

    let subnet_file = subnet_file_path();
    local_manager
        .handle_subnet_file(&subnet_file, &config, ip_masq, subnet, None, mtu)
        .with_context(|| {
            format!(
                "failed to write local subnet config to {}",
                subnet_file.as_str()
            )
        })?;

    info!(
        target: "rks::node::local",
        "local subnet config written to {}",
        subnet_file
    );

    Ok(())
}

async fn apply_routes(route_manager: Arc<Mutex<RouteManager>>, routes: Vec<Route>) {
    info!(
        target: "rks::node::local",
        "applying {} route entries for local rks node",
        routes.len()
    );

    let mut manager = route_manager.lock().await;
    let mut desired_v4_routes = Vec::new();

    for route in routes {
        match &route.dst {
            Some(IpNetwork::V4(_)) => {
                if !contains_route(&desired_v4_routes, &route) {
                    desired_v4_routes.push(route);
                }
            }
            Some(IpNetwork::V6(_)) => {
                debug!(
                    target: "rks::node::local",
                    "skip IPv6 route for local rks node: {route:?}"
                );
            }
            None => {
                warn!(
                    target: "rks::node::local",
                    "skip route without destination: {route:?}"
                );
            }
        }
    }

    let current_v4_routes = manager.get_routes().to_vec();

    let mut removed_v4 = 0usize;

    for stale_route in &current_v4_routes {
        if contains_route(&desired_v4_routes, stale_route) {
            continue;
        }

        if let Err(e) = manager.delete_route(stale_route).await {
            error!(
                target: "rks::node::local",
                "failed to remove stale IPv4 route {stale_route:?}: {e}"
            );
        } else {
            manager.remove_from_route_list(stale_route, AddressFamily::Inet);
            removed_v4 += 1;
        }
    }

    for route in desired_v4_routes {
        if let Err(e) = manager.add_route(&route).await {
            error!(
                target: "rks::node::local",
                "failed to add IPv4 route {route:?}: {e}"
            );
        }
    }

    info!(
        target: "rks::node::local",
        "route reconcile complete: desired_v4={}, removed_v4={}",
        manager.get_routes().len(),
        removed_v4
    );
}

fn contains_route(routes: &[Route], route: &Route) -> bool {
    routes
        .iter()
        .any(|existing| ip_route::route_equal(existing, route))
}

fn subnet_file_path() -> String {
    std::env::var("SUBNET_FILE_PATH").unwrap_or_else(|_| DEFAULT_SUBNET_FILE.to_string())
}

fn resolve_local_node_id(addr: &str) -> String {
    if let Ok(node_id) = std::env::var("RKS_NODE_NAME")
        && !node_id.trim().is_empty()
    {
        return node_id;
    }

    if let Ok(hostname) = std::env::var("HOSTNAME")
        && !hostname.trim().is_empty()
    {
        return format!("rks-{hostname}");
    }

    let suffix = sanitize_node_suffix(addr);
    format!("rks-{suffix}")
}

fn sanitize_node_suffix(raw: &str) -> String {
    let mut sanitized = raw
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();

    sanitized = sanitized.trim_matches('-').to_string();

    if sanitized.is_empty() {
        "node".to_string()
    } else {
        sanitized
    }
}
