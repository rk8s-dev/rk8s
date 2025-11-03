use crate::node::{Shared, WorkerSession};
use anyhow::Context;
use common::lease::{Lease, LeaseAttrs};
use common::quic::RksConnection;
use common::{Node, NodeNetworkConfig, RksMessage, log_error, reply_error_msg_and_bail};
use ipnetwork::{Ipv4Network, Ipv6Network};
use libnetwork::config::NetworkConfig;
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

        let session = Arc::new(WorkerSession::new(msg_tx, lease));
        self.shared
            .node_registry
            .register(node_id, session.clone())
            .await;

        self.conn.send_msg(&RksMessage::Ack).await?;

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
