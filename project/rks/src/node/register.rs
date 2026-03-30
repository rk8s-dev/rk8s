use crate::controllers::nftrules_controller::build_rules;
use crate::node::{Shared, WorkerSession};
use anyhow::Context;
use common::lease::{Lease, LeaseAttrs};
use common::quic::RksConnection;
use common::{Node, NodeNetworkConfig, RksMessage, log_error, reply_error_msg_and_bail};
use ipnetwork::{Ipv4Network, Ipv6Network};
use libnetwork::config::NetworkConfig;
use libnetwork::nftables::{
    generate_services_discovery_refresh, generate_verdict_maps_init_raw_json,
};
use log::info;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct NodeRegister<'a> {
    shared: &'a Shared,
    conn: &'a RksConnection,
}

impl<'a> NodeRegister<'a> {
    pub fn new(conn: &'a RksConnection, shared: &'a Shared) -> Self {
        Self { shared, conn }
    }

    pub async fn register(&self, node: Box<Node>) -> anyhow::Result<(bool, Option<String>)> {
        let id = node.metadata.name.clone();
        if id.is_empty() {
            reply_error_msg_and_bail!(
                self.conn,
                &RksMessage::Error("invalid node: metadata.name is empty".to_string())
            );
        }

        let lease = self.node_set_lease(&id).await?;

        let subnet = lease.subnet;
        let ipv6_subnet = lease.ipv6_subnet;

        self.register_node_in_registry(node, &id, lease).await?;

        let config = self.shared.local_manager.get_network_config().await?;
        info!("fetched network config: {config:?}");
        self.node_config_network(&id, &config, subnet, ipv6_subnet)
            .await?;

        Ok((true, Some(id)))
    }

    async fn node_set_lease(&self, node_id: impl Into<String>) -> anyhow::Result<Lease> {
        let node_id = node_id.into();

        let (public_ip, public_ipv6) = match self.conn.remote_address().ip() {
            IpAddr::V4(v4) => (v4, None),
            IpAddr::V6(v6) => (Ipv4Addr::new(0, 0, 0, 0), Some(v6)),
        };

        let lease_attrs = LeaseAttrs {
            public_ip,
            public_ipv6,
            backend_type: "hostgw".to_string(),
            node_id,
            ..Default::default()
        };
        let lease = self
            .shared
            .local_manager
            .acquire_lease(&lease_attrs)
            .await?;

        info!("acquired worker node lease: {lease:?}");
        Ok(lease)
    }

    async fn register_node_in_registry(
        &self,
        mut node: Box<Node>,
        node_id: impl Into<String>,
        lease: Lease,
    ) -> anyhow::Result<()> {
        let node_id = node_id.into();
        let subnet = lease.subnet;

        let (msg_tx, mut msg_rx) = mpsc::channel::<RksMessage>(32);

        node.spec.pod_cidr = subnet.to_string();
        self.shared.xline_store.insert_node(&node).await?;

        info!(
            "registered worker node: {node_id}, ip: {}",
            self.conn.remote_address().ip()
        );

        // Register worker before bootstrap rule generation so incremental
        // broadcasts during this window can still reach this session.
        let session = Arc::new(WorkerSession::new(msg_tx.clone(), lease));
        self.shared
            .node_registry
            .register(node_id.clone(), session.clone())
            .await;

        // Send current nftables rules in two phases:
        // 1) full ruleset (contains table flush)
        // 2) verdict map initialization (raw JSON)
        match build_rules(&self.shared.xline_store).await {
            Ok(rules) => {
                info!(
                    "initial nftables send phase=full_rules node={} payload_len={}",
                    node_id,
                    rules.len()
                );
                let full_msg = RksMessage::SetNftablesRules(rules);
                if let Err(e) = msg_tx.try_send(full_msg) {
                    log::warn!(
                        "Failed to send initial nftables rules to {}: {}",
                        node_id,
                        e
                    );
                }

                match generate_verdict_maps_init_raw_json() {
                    Ok(map_init_rules) => {
                        info!(
                            "initial nftables send phase=map_init node={} payload_len={}",
                            node_id,
                            map_init_rules.len()
                        );
                        let map_init_msg = RksMessage::SetNftablesRules(map_init_rules);
                        if let Err(e) = msg_tx.try_send(map_init_msg) {
                            log::warn!(
                                "Failed to send initial verdict-map init rules to {}: {}",
                                node_id,
                                e
                            );
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to generate initial verdict-map init rules for {}: {}",
                            node_id,
                            e
                        );
                    }
                }

                match self.shared.xline_store.services_snapshot_with_rev().await {
                    Ok((services_raw, _)) => {
                        let mut services = Vec::new();
                        for (key, yaml) in services_raw {
                            match serde_yaml::from_str::<common::ServiceTask>(&yaml) {
                                Ok(svc) => services.push(svc),
                                Err(e) => log::warn!(
                                    "Failed to parse Service {} while building initial discovery refresh: {}",
                                    key,
                                    e
                                ),
                            }
                        }

                        match generate_services_discovery_refresh(&services) {
                            Ok(discovery_rules) => {
                                info!(
                                    "initial nftables send phase=discovery_refresh node={} payload_len={}",
                                    node_id,
                                    discovery_rules.len()
                                );
                                let msg = RksMessage::UpdateNftablesRules(discovery_rules);
                                if let Err(e) = msg_tx.try_send(msg) {
                                    log::warn!(
                                        "Failed to send initial discovery refresh rules to {}: {}",
                                        node_id,
                                        e
                                    );
                                }
                            }
                            Err(e) => log::warn!(
                                "Failed to generate initial discovery refresh rules for {}: {}",
                                node_id,
                                e
                            ),
                        }
                    }
                    Err(e) => log::warn!(
                        "Failed to read services snapshot while building initial discovery refresh for {}: {}",
                        node_id,
                        e
                    ),
                }
            }
            Err(e) => {
                log::error!(
                    "Failed to build initial nftables rules for {}: {}",
                    node_id,
                    e
                );
            }
        }

        // Push credentials when we have an authoritative snapshot. If vault
        // is not configured, send an empty list to clear stale worker state.
        // If vault read fails, skip the update so we do not overwrite a valid
        // in-memory cache with an accidental empty snapshot.
        let initial_registry_credentials = match self.shared.vault.as_ref() {
            Some(vault) => match vault.list_registry_credentials().await {
                Ok(creds) => Some(creds),
                Err(e) => {
                    log::warn!(
                        "Failed to load registry credentials for {}: {}; skipping credential sync",
                        node_id,
                        e
                    );
                    None
                }
            },
            None => Some(Vec::new()),
        };
        if let Some(initial_registry_credentials) = initial_registry_credentials {
            if !initial_registry_credentials.is_empty()
                && !crate::protocol::config::config_ref().tls_config.enable
            {
                log::warn!(
                    "Sending {} registry credential(s) to {node_id} over non-TLS QUIC; \
                     enable TLS in production to protect secrets in transit",
                    initial_registry_credentials.len(),
                );
            }
            let msg = RksMessage::SetRegistryCredentials(initial_registry_credentials);
            if let Err(e) = msg_tx.try_send(msg) {
                log::warn!("Failed to send registry credentials to {}: {}", node_id, e);
            }
        }

        if let Err(e) = self.conn.send_msg(&RksMessage::Ack).await {
            self.shared
                .node_registry
                .unregister_if_matches(&node_id, &session)
                .await;
            return Err(e);
        }

        let conn = self.conn.clone();
        tokio::spawn(async move {
            while let Some(msg) = msg_rx.recv().await {
                log_error!(conn.send_msg(&msg).await)
            }
        });
        Ok(())
    }

    async fn node_config_network(
        &self,
        node_id: impl Into<String>,
        config: &NetworkConfig,
        ipv4_subnet: Ipv4Network,
        ipv6_subnet: Option<Ipv6Network>,
    ) -> anyhow::Result<()> {
        let node_id = node_id.into();

        let node_net_config = build_node_network_config(
            node_id.clone(),
            config,
            false,
            Some(ipv4_subnet),
            ipv6_subnet,
        )?;

        let msg = RksMessage::SetNetwork(Box::new(node_net_config));

        let worker = self
            .shared
            .node_registry
            .get(&node_id)
            .await
            .with_context(|| format!("No active worker for {node_id}"))?;
        worker
            .tx
            .try_send(msg)
            .with_context(|| format!("Failed to enqueue message for {node_id}"))
    }
}

/// Build node network configuration environment variables.
pub fn build_node_network_config(
    node_id: String,
    config: &NetworkConfig,
    ip_masq: bool,
    mut sn4: Option<Ipv4Network>,
    mut sn6: Option<Ipv6Network>,
) -> anyhow::Result<NodeNetworkConfig> {
    let mut contents = String::new();

    if config.enable_ipv4
        && let Some(ref mut net) = sn4
    {
        contents += &format!(
            "RKL_NETWORK={}\n",
            config.network.context("IPv4 network config missing")?
        );
        contents += &format!("RKL_SUBNET={}/{}\n", net.ip(), net.prefix());
    }

    if config.enable_ipv6
        && let Some(ref mut net) = sn6
    {
        contents += &format!(
            "RKL_IPV6_NETWORK={}\n",
            config.ipv6_network.context("IPv6 network config missing")?
        );
        contents += &format!("RKL_IPV6_SUBNET={}/{}\n", net.ip(), net.prefix());
    }

    contents += &format!("RKL_IPMASQ={ip_masq}\n");

    Ok(NodeNetworkConfig {
        node_id,
        subnet_env: contents,
    })
}
