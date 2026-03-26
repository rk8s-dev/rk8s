use crate::api::xlinestore::XlineStore;
use crate::network::manager::LocalManager;
use crate::node::lease_sync::LeaseSynchronizer;
use crate::node::server::QUICServer;
use crate::vault::Vault;
use common::RksMessage;
use common::lease::Lease;
use log::info;
use log::warn;
use nftables::{batch::Batch, schema, types};
use serde_json::json;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify, mpsc};

pub mod cert;
mod dispatch;
mod heartbeat;
mod lease_sync;
mod local_node;
mod register;
mod server;
mod watcher;

#[derive(Clone)]
pub struct WorkerSession {
    pub tx: mpsc::Sender<RksMessage>,
    pub cancel_notify: Arc<Notify>,
    pub lease: Arc<Mutex<Lease>>,
}

impl WorkerSession {
    pub fn new(tx: mpsc::Sender<RksMessage>, lease: Lease) -> Self {
        Self {
            tx,
            lease: Arc::new(Mutex::new(lease)),
            cancel_notify: Arc::new(Notify::new()),
        }
    }
}

#[derive(Default)]
pub struct NodeRegistry {
    inner: Mutex<HashMap<String, Arc<WorkerSession>>>,
}

#[allow(unused)]
impl NodeRegistry {
    pub async fn register(&self, node_id: String, session: Arc<WorkerSession>) {
        let mut inner = self.inner.lock().await;
        inner.insert(node_id, session);
    }

    pub async fn unregister(&self, node_id: &str) {
        let session = {
            let mut inner = self.inner.lock().await;
            inner.remove(node_id)
        };

        if let Some(session) = session {
            Self::cleanup_session(node_id, session);
        }
    }

    pub async fn unregister_if_matches(
        &self,
        node_id: &str,
        expected: &Arc<WorkerSession>,
    ) -> bool {
        let session = {
            let mut inner = self.inner.lock().await;
            match inner.get(node_id) {
                Some(current) if Arc::ptr_eq(current, expected) => inner.remove(node_id),
                _ => None,
            }
        };

        if let Some(session) = session {
            Self::cleanup_session(node_id, session);
            return true;
        }

        false
    }

    pub async fn get(&self, node_id: &str) -> Option<Arc<WorkerSession>> {
        let inner = self.inner.lock().await;
        inner.get(node_id).cloned()
    }

    /// Return a snapshot of all registered worker sessions.
    pub async fn list_sessions(&self) -> Vec<(String, Arc<WorkerSession>)> {
        let inner = self.inner.lock().await;
        inner.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    fn cleanup_session(node_id: &str, session: Arc<WorkerSession>) {
        let cleanup_rules = build_delete_table_ruleset();
        if let Err(e) = session
            .tx
            .try_send(RksMessage::SetNftablesRules(cleanup_rules))
        {
            warn!("Failed to send nftables cleanup to node {}: {}", node_id, e);
        }

        session.cancel_notify.notify_one();
    }
}

/// Registry that maps a pod log request key to a channel sender,
/// so that log chunks arriving from a worker can be forwarded to the waiting user connection.
#[derive(Default)]
pub struct LogResponseRegistry {
    inner: Mutex<HashMap<String, mpsc::Sender<RksMessage>>>,
}

impl LogResponseRegistry {
    pub async fn register(&self, key: String) -> mpsc::Receiver<RksMessage> {
        let (tx, rx) = mpsc::channel(64);
        self.inner.lock().await.insert(key, tx);
        rx
    }

    pub async fn send(&self, key: &str, msg: RksMessage) {
        let inner = self.inner.lock().await;
        if let Some(tx) = inner.get(key) {
            let _ = tx.try_send(msg);
        }
    }
}

fn build_delete_table_ruleset() -> String {
    // Use nftables batch builder for consistency with the rest of the codebase
    let mut batch = Batch::new();
    batch.delete(schema::NfListObject::Table(schema::Table {
        family: types::NfFamily::IP,
        name: Cow::Borrowed("rk8s"),
        ..Default::default()
    }));

    serde_json::to_string(&batch.to_nftables()).unwrap_or_else(|e| {
        warn!("Failed to serialize nft delete-table ruleset: {}", e);
        // Fallback to an empty ruleset string; unregister will still proceed
        json!({ "nftables": [] }).to_string()
    })
}

pub struct RksNode {
    addr: String,
    shared: Arc<Shared>,
}

impl RksNode {
    pub fn new(addr: String, shared: Arc<Shared>) -> Self {
        Self { addr, shared }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        info!("Starting server with address: {}", self.addr);

        self.start_background_tasks();

        let server = QUICServer::new(self.addr.parse()?, self.shared.vault.clone()).await?;
        server.serve(self.shared.clone()).await
    }

    fn start_background_tasks(&self) {
        // Check if lastheartbeattime times out
        heartbeat::watch(
            self.shared.xline_store.clone(),
            Duration::from_secs(50), // grace
            Duration::from_secs(10), // interval
        );
        info!("Heartbeat monitor started");

        let shared_clone = self.shared.clone();
        let addr_clone = self.addr.clone();
        tokio::spawn(async move {
            if let Err(e) = local_node::bootstrap(shared_clone, &addr_clone).await {
                warn!("Failed to bootstrap local rks node networking: {e:#}");
            }
        });

        // Spawn task to propagate lease updates to workers
        LeaseSynchronizer::spawn(
            self.shared.local_manager.clone(),
            self.shared.node_registry.clone(),
        );
        info!("Lease synchronizer started");
    }
}

pub struct Shared {
    pub xline_store: Arc<XlineStore>,
    pub local_manager: Arc<LocalManager>,
    pub vault: Option<Arc<Vault>>,
    pub node_registry: Arc<NodeRegistry>,
    pub network_config: Arc<libnetwork::config::NetworkConfig>,
    pub service_ip_allocator: Option<Arc<crate::network::service_ip::ServiceIpAllocator>>,
    pub log_response_registry: Arc<LogResponseRegistry>,
}

impl Shared {
    pub fn new(
        xline_store: Arc<XlineStore>,
        local_manager: Arc<LocalManager>,
        vault: Option<Arc<Vault>>,
        node_registry: Arc<NodeRegistry>,
        network_config: Arc<libnetwork::config::NetworkConfig>,
        service_ip_allocator: Option<Arc<crate::network::service_ip::ServiceIpAllocator>>,
    ) -> Self {
        Self {
            xline_store,
            local_manager,
            vault,
            node_registry,
            network_config,
            service_ip_allocator,
            log_response_registry: Arc::new(LogResponseRegistry::default()),
        }
    }
}
