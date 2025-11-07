use crate::api::xlinestore::XlineStore;
use crate::network::manager::LocalManager;
use crate::node::lease_sync::LeaseSynchronizer;
use crate::node::server::QUICServer;
use crate::vault::Vault;
use common::RksMessage;
use common::lease::Lease;
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify, mpsc};

pub mod cert;
mod dispatch;
mod heartbeat;
mod lease_sync;
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
        let mut inner = self.inner.lock().await;
        if let Some(session) = inner.remove(node_id) {
            session.cancel_notify.notify_one();
        }
    }

    pub async fn get(&self, node_id: &str) -> Option<Arc<WorkerSession>> {
        let inner = self.inner.lock().await;
        inner.get(node_id).cloned()
    }
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
}

impl Shared {
    pub fn new(
        xline_store: Arc<XlineStore>,
        local_manager: Arc<LocalManager>,
        vault: Option<Arc<Vault>>,
        node_registry: Arc<NodeRegistry>,
    ) -> Self {
        Self {
            xline_store,
            local_manager,
            vault,
            node_registry,
        }
    }
}
