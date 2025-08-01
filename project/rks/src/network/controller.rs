use crate::network::{
    config::{self, Config}, ip::{next_ipv4_network, next_ipv6_network}, lease::{EventType, Lease, LeaseAttrs, LeaseWatchResult}, manager::LocalManager, registry::{Registry, XlineRegistryError}, subnet 
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Controller {
    manager: LocalManager,
    // 记录每个节点的分配
    node_subnets: Arc<Mutex<HashMap<String, Lease>>>,
}

impl Controller {
    pub async fn register_node(&mut self, node_id: String, attrs: LeaseAttrs) -> anyhow::Result<()> {
        // 集中式分配
        let lease = self.manager.acquire_lease(&attrs).await?;
        // 记录分配
        self.node_subnets.lock().await.insert(node_id.clone(),  lease.clone());
        // 下发配置（如通过 gRPC/HTTP/QUIC）
        self.push_subnet_config_to_node(node_id, &lease).await?;
        Ok(())
    }

    async fn push_subnet_config_to_node(&self, node_id: String, lease: &Lease) -> anyhow::Result<()> {
        // 这里实现 gRPC/HTTP/QUIC 下发
        // 例如 HTTP POST http://node_ip:port/api/subnet_config
        // 或通过消息队列推送
        Ok(())
    }
}