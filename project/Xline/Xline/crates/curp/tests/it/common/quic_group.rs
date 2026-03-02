//! QUIC-based CurpGroup for integration testing
//!
//! This module provides a QUIC variant of `CurpGroup` that uses gm-quic
//! instead of tonic gRPC for transport. All nodes share a single `QuicListeners`
//! (gm-quic global singleton) with per-node virtual hosts routed by SNI.

use std::{collections::HashMap, sync::Arc, time::Duration};

use curp::{
    client::{ClientApi, ClientBuilder},
    members::{ClusterInfo, ServerId},
    rpc::{FetchClusterRequest, FetchClusterResponse, MethodId, QuicChannel, QuicGrpcServer},
    server::{
        DB, Rpc,
        conflict::test_pools::{TestSpecPool, TestUncomPool},
    },
};
use curp_test_utils::{
    TestRoleChange, TestRoleChangeInner,
    test_cmd::{TestCE, TestCommand, TestCommandResult},
};
use engine::MemorySnapshotAllocator;
use gm_quic::prelude::{QuicClient, QuicListeners};
use rcgen::{CertificateParams, KeyPair};
use rustls::pki_types::CertificateDer;
use tokio::{net::TcpListener, sync::mpsc, task::JoinHandle};
use tracing::debug;
use utils::{
    config::{ClientConfig, CurpConfig, EngineConfig},
    task_manager::TaskManager,
};

/// A QUIC-based test node
pub struct QuicCurpNode {
    pub id: ServerId,
    pub addr: String,
    pub server_name: String,
    pub exe_rx: mpsc::UnboundedReceiver<(TestCommand, TestCommandResult)>,
    pub as_rx: mpsc::UnboundedReceiver<(TestCommand, curp::LogIndex)>,
    pub role_change_arc: Arc<TestRoleChangeInner>,
    pub task_manager: Arc<TaskManager>,
}

/// Self-signed CA + per-node certificates for testing
struct TestCerts {
    ca_cert_der: CertificateDer<'static>,
    nodes: Vec<(String, Vec<u8>, Vec<u8>)>,
}

impl TestCerts {
    fn generate(server_names: &[String]) -> Self {
        let ca_key = KeyPair::generate().expect("generate CA key");
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("CA params");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_cert = ca_params.self_signed(&ca_key).expect("self-sign CA");
        let ca_cert_der = CertificateDer::from(ca_cert.der().to_vec());

        let mut nodes = Vec::new();
        for name in server_names {
            let node_key = KeyPair::generate().expect("generate node key");
            let mut node_params =
                CertificateParams::new(vec![name.clone()]).expect("node cert params");
            node_params.is_ca = rcgen::IsCa::NoCa;
            let node_cert = node_params
                .signed_by(&node_key, &ca_cert, &ca_key)
                .expect("sign node cert");
            nodes.push((
                name.clone(),
                node_cert.der().to_vec(),
                node_key.serialize_der(),
            ));
        }

        Self { ca_cert_der, nodes }
    }
}

/// QUIC-based CurpGroup for integration testing
pub struct QuicCurpGroup {
    pub nodes: HashMap<ServerId, QuicCurpNode>,
    pub quic_client: Arc<QuicClient>,
    /// Keep a reference to the listeners so we can shut them down on drop
    listeners: Arc<QuicListeners>,
    _accept_handle: JoinHandle<()>,
}

impl QuicCurpGroup {
    /// Create a new QUIC-based CurpGroup with `n_nodes` nodes.
    ///
    /// Uses a single shared `QuicListeners` (gm-quic global singleton) with
    /// per-node virtual hosts. A custom accept loop dispatches connections
    /// to the correct `QuicGrpcServer` based on the SNI server_name.
    pub async fn new(n_nodes: usize) -> Self {
        assert!(n_nodes >= 3, "the number of nodes must >= 3");

        let server_names: Vec<String> = (0..n_nodes).map(|i| format!("s{i}.test")).collect();
        let node_names: Vec<String> = (0..n_nodes).map(|i| format!("S{i}")).collect();

        // Allocate free UDP ports
        let mut ports = Vec::new();
        for _ in 0..n_nodes {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            ports.push(port);
        }

        let certs = TestCerts::generate(&server_names);

        // Build the single shared QuicListeners with all virtual hosts
        let listeners = QuicListeners::builder()
            .expect("QuicListeners::builder")
            .without_client_cert_verifier()
            .listen(64);

        for (i, (sni, cert_der, key_der)) in certs.nodes.iter().enumerate() {
            let bind_uri_str = format!("inet://127.0.0.1:{}", ports[i]);
            listeners
                .add_server(
                    sni.as_str(),
                    cert_der.as_slice(),
                    key_der.as_slice(),
                    vec![
                        bind_uri_str
                            .parse::<gm_quic::qbase::net::addr::BindUri>()
                            .unwrap(),
                    ],
                    None::<Vec<u8>>,
                )
                .expect("add_server");
        }

        // Build a shared QuicClient that trusts our CA
        let mut root_store = rustls::RootCertStore::empty();
        root_store
            .add(certs.ca_cert_der.clone())
            .expect("add CA cert");
        let quic_client = Arc::new(
            QuicClient::builder()
                .with_root_certificates(root_store)
                .without_cert()
                .build(),
        );

        // Build all_members_addrs for ClusterInfo
        // Use "server_name:port" — gm-quic resolves server_name via DNS,
        // and the SNI matches the certificate
        let all_members_addrs: HashMap<String, Vec<String>> = node_names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                (
                    name.clone(),
                    vec![format!("{}:{}", server_names[i], ports[i])],
                )
            })
            .collect();

        let leader_name = "S0";
        let mut nodes = HashMap::new();

        // Build per-node QuicGrpcServer instances, keyed by server_name
        let mut servers: HashMap<String, QuicGrpcServer<TestCommand, TestCE, TestRoleChange>> =
            HashMap::new();

        for (i, name) in node_names.iter().enumerate() {
            let config = Arc::new(CurpConfig::default());
            let task_manager = Arc::new(TaskManager::new());
            let snapshot_allocator =
                Box::<MemorySnapshotAllocator>::default() as Box<dyn engine::SnapshotAllocator>;
            let cluster_info = Arc::new(ClusterInfo::from_members_map(
                all_members_addrs.clone(),
                [],
                name,
            ));
            let id = cluster_info.self_id();

            let (exe_tx, exe_rx) = mpsc::unbounded_channel();
            let (as_tx, as_rx) = mpsc::unbounded_channel();
            let ce = Arc::new(TestCE::new(
                name.clone(),
                exe_tx,
                as_tx,
                EngineConfig::default(),
            ));

            let role_change_cb = TestRoleChange::default();
            let role_change_arc = role_change_cb.get_inner_arc();
            let curp_storage = Arc::new(DB::open(&config.engine_cfg).unwrap());

            let rpc = Rpc::new_with_quic_for_test(
                cluster_info,
                name == leader_name,
                ce,
                snapshot_allocator,
                role_change_cb,
                config,
                curp_storage,
                Arc::clone(&task_manager),
                None,
                vec![Box::<TestSpecPool>::default()],
                vec![Box::<TestUncomPool>::default()],
                Arc::clone(&quic_client),
            );

            servers.insert(server_names[i].clone(), QuicGrpcServer::new(rpc));

            let addr = format!("{}:{}", server_names[i], ports[i]);
            nodes.insert(
                id,
                QuicCurpNode {
                    id,
                    addr,
                    server_name: server_names[i].clone(),
                    exe_rx,
                    as_rx,
                    role_change_arc,
                    task_manager,
                },
            );
        }

        // Spawn a single accept loop that dispatches by server_name
        let listeners_clone = Arc::clone(&listeners);
        let accept_handle = tokio::spawn(async move {
            loop {
                match listeners_clone.accept().await {
                    Ok((conn, sni, _pathway, _link)) => {
                        if let Some(server) = servers.get(&sni) {
                            let _ = server.spawn_connection(conn);
                        } else {
                            debug!("no server for SNI: {sni}");
                        }
                    }
                    Err(e) => {
                        debug!("listeners shutdown: {e}");
                        break;
                    }
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(300)).await;
        debug!("successfully started QUIC group");

        Self {
            nodes,
            quic_client,
            listeners,
            _accept_handle: accept_handle,
        }
    }

    pub fn all_addrs(&self) -> impl Iterator<Item = &String> {
        self.nodes.values().map(|n| &n.addr)
    }

    pub fn all_addrs_map(&self) -> HashMap<ServerId, Vec<String>> {
        self.nodes
            .iter()
            .map(|(id, n)| (*id, vec![n.addr.clone()]))
            .collect()
    }

    /// Create a QUIC-based client
    pub async fn new_client(
        &self,
    ) -> impl ClientApi<Error = tonic::Status, Cmd = TestCommand> + use<> {
        let addrs: Vec<String> = self.all_addrs().cloned().collect();
        ClientBuilder::new(ClientConfig::default(), true)
            .quic_transport_for_test(Arc::clone(&self.quic_client))
            .quic_discover_from(addrs)
            .await
            .unwrap()
            .build()
            .unwrap()
    }

    /// Try to get the current leader by querying all nodes via QUIC
    pub async fn try_get_leader(&self) -> Option<(ServerId, u64)> {
        for node in self.nodes.values() {
            let channel = match QuicChannel::connect_single_for_test(
                &node.addr,
                Arc::clone(&self.quic_client),
            )
            .await
            {
                Ok(ch) => ch,
                Err(_) => continue,
            };

            let result: Result<FetchClusterResponse, curp::rpc::CurpError> = channel
                .unary_call(
                    MethodId::FetchCluster,
                    FetchClusterRequest::default(),
                    vec![],
                    Duration::from_secs(5),
                )
                .await;

            if let Ok(resp) = result {
                if let Some(leader) = resp.leader_id {
                    return Some((leader.value, resp.term));
                }
            }
        }
        None
    }

    /// Get the leader, retrying until found
    pub async fn get_leader(&self) -> (ServerId, u64) {
        for _ in 0..30 {
            if let Some(leader) = self.try_get_leader().await {
                return leader;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        panic!("failed to find leader after retries");
    }

    pub fn get_node(&self, id: &ServerId) -> &QuicCurpNode {
        &self.nodes[id]
    }

    pub fn get_node_mut(&mut self, id: &ServerId) -> &mut QuicCurpNode {
        self.nodes.get_mut(id).unwrap()
    }

    pub fn exe_rxs(
        &mut self,
    ) -> impl Iterator<Item = &mut mpsc::UnboundedReceiver<(TestCommand, TestCommandResult)>> {
        self.nodes.values_mut().map(|node| &mut node.exe_rx)
    }

    pub fn as_rxs(
        &mut self,
    ) -> impl Iterator<Item = &mut mpsc::UnboundedReceiver<(TestCommand, curp::LogIndex)>> {
        self.nodes.values_mut().map(|node| &mut node.as_rx)
    }

    /// Explicitly shut down the group (async, waits for cleanup).
    ///
    /// Prefer calling this at the end of each test instead of relying on Drop.
    pub async fn close(self) {
        // Shut down listeners first so the global singleton is released
        self.listeners.shutdown();
        self._accept_handle.abort();
        for node in self.nodes.values() {
            node.task_manager.shutdown(true).await;
        }
        // Small grace period for background tasks to finish
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

impl Drop for QuicCurpGroup {
    fn drop(&mut self) {
        // Best-effort cleanup if close() was not called.
        // listeners.shutdown() and abort() are synchronous.
        self.listeners.shutdown();
        self._accept_handle.abort();
        // We cannot await async shutdown in Drop, but we can fire-and-forget
        // the shutdown signal. The serial_test attribute ensures no overlap.
    }
}
