use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use curp::{
    LogIndex,
    client::{ClientApi, ClientBuilder},
    error::ServerError,
    members::{ClusterInfo, ServerId},
    rpc::{
        FetchClusterRequest, FetchClusterResponse, Member, MethodId, QuicChannel, QuicGrpcServer,
    },
    server::{
        DB, Rpc,
        conflict::test_pools::{TestSpecPool, TestUncomPool},
    },
};
use curp_test_utils::{
    TestRoleChange, TestRoleChangeInner,
    test_cmd::{TestCE, TestCommand, TestCommandResult},
};
use engine::{EngineType, MemorySnapshotAllocator, RocksSnapshotAllocator, SnapshotAllocator};
use futures::future::join_all;
use gm_quic::prelude::{QuicClient, QuicListeners};
use itertools::Itertools;
use rcgen::{CertificateParams, KeyPair};
use rustls::pki_types::CertificateDer;
use tokio::{
    net::TcpListener,
    runtime::Handle,
    sync::mpsc,
    task::{JoinHandle, block_in_place},
    time::timeout,
};
use tracing::debug;
use utils::{
    config::{ClientConfig, CurpConfig, EngineConfig},
    task_manager::{TaskManager, tasks::TaskName},
};
use xlinerpc::status::Status;

/// `BOTTOM_TASKS` are tasks which not dependent on other tasks in the task group.
const BOTTOM_TASKS: [TaskName; 2] = [TaskName::WatchTask, TaskName::ConfChange];

/// The default shutdown timeout used in `wait_for_targets_shutdown`
pub(crate) const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(7);

type ServerMap = HashMap<String, QuicGrpcServer<TestCommand, TestCE, TestRoleChange>>;

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

pub struct CurpNode {
    pub id: ServerId,
    pub addr: String,
    pub server_name: String,
    pub exe_rx: mpsc::UnboundedReceiver<(TestCommand, TestCommandResult)>,
    pub as_rx: mpsc::UnboundedReceiver<(TestCommand, LogIndex)>,
    pub role_change_arc: Arc<TestRoleChangeInner>,
    pub task_manager: Arc<TaskManager>,
}

pub struct CurpGroup {
    pub nodes: HashMap<ServerId, CurpNode>,
    pub storage_path: Option<PathBuf>,
    pub quic_client: Arc<QuicClient>,
    listeners: Arc<QuicListeners>,
    /// Shared server map for the accept loop and run_node
    servers: Arc<tokio::sync::RwLock<ServerMap>>,
    _accept_handle: JoinHandle<()>,
}

impl CurpGroup {
    pub async fn new(n_nodes: usize) -> Self {
        let configs = (0..n_nodes)
            .map(|i| (format!("S{i}"), Default::default()))
            .collect();
        Self::new_with_configs(configs, "S0".to_owned()).await
    }

    pub async fn new_rocks(n_nodes: usize, path: PathBuf) -> Self {
        let configs = (0..n_nodes)
            .map(|i| {
                let name = format!("S{i}");
                let mut config = CurpConfig::default();
                let dir = path.join(&name);
                config.engine_cfg = EngineConfig::RocksDB(dir.join("curp"));
                let xline_storage_config = EngineConfig::RocksDB(dir.join("xline"));
                (name, (Arc::new(config), xline_storage_config))
            })
            .collect();
        let mut inner = Self::new_with_configs(configs, "S0".to_owned()).await;
        inner.storage_path = Some(path);
        inner
    }

    async fn new_with_configs(
        configs: HashMap<String, (Arc<CurpConfig>, EngineConfig)>,
        leader_name: String,
    ) -> Self {
        let n_nodes = configs.len();
        assert!(n_nodes >= 3, "the number of nodes must >= 3");

        let node_names: Vec<String> = configs.keys().cloned().sorted().collect();
        let server_names: Vec<String> = node_names
            .iter()
            .enumerate()
            .map(|(i, _)| format!("s{i}.test"))
            .collect();

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
                .bind(["inet://0.0.0.0:0"])
                .with_alpns(["h3"])
                .build(),
        );

        // Build all_members_addrs for ClusterInfo
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

        let mut nodes = HashMap::new();
        let mut server_map: ServerMap = HashMap::new();

        for (idx, name) in node_names.iter().enumerate() {
            let (config, xline_storage_config) = configs.get(name).unwrap().clone();
            let task_manager = Arc::new(TaskManager::new());
            let snapshot_allocator = Self::get_snapshot_allocator_from_cfg(&config);
            let cluster_info = Arc::new(ClusterInfo::from_members_map(
                all_members_addrs.clone(),
                &[],
                name,
            ));
            let id = cluster_info.self_id();

            let (exe_tx, exe_rx) = mpsc::unbounded_channel();
            let (as_tx, as_rx) = mpsc::unbounded_channel();
            let ce = Arc::new(TestCE::new(
                name.clone(),
                exe_tx,
                as_tx,
                xline_storage_config,
            ));

            let role_change_cb = TestRoleChange::default();
            let role_change_arc = role_change_cb.get_inner_arc();
            let curp_storage = Arc::new(DB::open(&config.engine_cfg).unwrap());
            let rpc = Rpc::new_for_test(
                cluster_info,
                *name == leader_name,
                ce,
                snapshot_allocator,
                role_change_cb,
                config,
                curp_storage,
                Arc::clone(&task_manager),
                vec![Box::<TestSpecPool>::default()],
                vec![Box::<TestUncomPool>::default()],
                Arc::clone(&quic_client),
            )
            .unwrap();

            server_map.insert(server_names[idx].clone(), QuicGrpcServer::new(rpc));

            let addr = format!("{}:{}", server_names[idx], ports[idx]);
            nodes.insert(
                id,
                CurpNode {
                    id,
                    addr,
                    server_name: server_names[idx].clone(),
                    exe_rx,
                    as_rx,
                    role_change_arc,
                    task_manager,
                },
            );
        }

        let servers = Arc::new(tokio::sync::RwLock::new(server_map));

        // Spawn a single accept loop that dispatches by server_name
        let listeners_clone = Arc::clone(&listeners);
        let servers_clone = Arc::clone(&servers);
        let accept_handle = tokio::spawn(async move {
            loop {
                match listeners_clone.accept().await {
                    Ok((conn, sni, _pathway, _link)) => {
                        let servers_r = servers_clone.read().await;
                        if let Some(server) = servers_r.get(&sni) {
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
        debug!("successfully start group");
        Self {
            nodes,
            storage_path: None,
            quic_client,
            listeners,
            servers,
            _accept_handle: accept_handle,
        }
    }

    fn get_snapshot_allocator_from_cfg(config: &CurpConfig) -> Box<dyn SnapshotAllocator> {
        match config.engine_cfg {
            EngineConfig::Memory => {
                Box::<MemorySnapshotAllocator>::default() as Box<dyn SnapshotAllocator>
            }
            EngineConfig::RocksDB(_) => {
                Box::<RocksSnapshotAllocator>::default() as Box<dyn SnapshotAllocator>
            }
            _ => unreachable!(),
        }
    }

    pub async fn run_node(
        &mut self,
        _listener: TcpListener,
        name: String,
        cluster_info: Arc<ClusterInfo>,
    ) {
        self.run_node_with_config(
            _listener,
            name,
            cluster_info,
            Arc::new(CurpConfig::default()),
            EngineConfig::default(),
        )
        .await
    }

    pub async fn run_node_with_config(
        &mut self,
        _listener: TcpListener,
        name: String,
        cluster_info: Arc<ClusterInfo>,
        config: Arc<CurpConfig>,
        xline_storage_config: EngineConfig,
    ) {
        let task_manager = Arc::new(TaskManager::new());
        let snapshot_allocator = Self::get_snapshot_allocator_from_cfg(&config);

        let (exe_tx, exe_rx) = mpsc::unbounded_channel();
        let (as_tx, as_rx) = mpsc::unbounded_channel();
        let ce = Arc::new(TestCE::new(
            name.clone(),
            exe_tx,
            as_tx,
            xline_storage_config,
        ));

        let id = cluster_info.self_id();
        let addr = cluster_info.self_peer_urls().pop().unwrap();
        let role_change_cb = TestRoleChange::default();
        let role_change_arc = role_change_cb.get_inner_arc();
        let curp_storage = Arc::new(DB::open(&config.engine_cfg).unwrap());
        let rpc = Rpc::new_for_test(
            cluster_info,
            false,
            ce,
            snapshot_allocator,
            role_change_cb,
            config,
            curp_storage,
            Arc::clone(&task_manager),
            vec![],
            vec![],
            Arc::clone(&self.quic_client),
        )
        .unwrap();

        // Extract server_name from addr (the hostname part before ':')
        let server_name = addr.split(':').next().unwrap_or(&addr).to_owned();

        // Generate a cert for this new server_name and add to listeners
        let certs = TestCerts::generate(&[server_name.clone()]);
        let (_, cert_der, key_der) = &certs.nodes[0];
        let port: u16 = addr.split(':').nth(1).unwrap().parse().unwrap();
        let bind_uri_str = format!("inet://127.0.0.1:{port}");
        self.listeners
            .add_server(
                &server_name,
                cert_der.as_slice(),
                key_der.as_slice(),
                vec![
                    bind_uri_str
                        .parse::<gm_quic::qbase::net::addr::BindUri>()
                        .unwrap(),
                ],
                None::<Vec<u8>>,
            )
            .expect("add_server for new node");

        // Add the QuicGrpcServer for this node to the shared map
        {
            let mut servers_w = self.servers.write().await;
            servers_w.insert(server_name.clone(), QuicGrpcServer::new(rpc));
        }

        self.nodes.insert(
            id,
            CurpNode {
                id,
                addr,
                server_name,
                exe_rx,
                as_rx,
                role_change_arc,
                task_manager,
            },
        );
        let client = self.new_client().await;
        client.propose_publish(id, name, vec![]).await.unwrap();
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

    pub fn get_node(&self, id: &ServerId) -> &CurpNode {
        &self.nodes[id]
    }

    pub fn get_node_mut(&mut self, id: &ServerId) -> &mut CurpNode {
        self.nodes.get_mut(id).unwrap()
    }

    pub async fn new_client(&self) -> impl ClientApi<Error = Status, Cmd = TestCommand> + use<> {
        let addrs: Vec<String> = self.all_addrs().cloned().collect();
        ClientBuilder::new(ClientConfig::default(), true)
            .quic_transport_for_test(Arc::clone(&self.quic_client))
            .discover_from(addrs)
            .await
            .unwrap()
            .build()
            .unwrap()
    }

    pub fn exe_rxs(
        &mut self,
    ) -> impl Iterator<Item = &mut mpsc::UnboundedReceiver<(TestCommand, TestCommandResult)>> {
        self.nodes.values_mut().map(|node| &mut node.exe_rx)
    }

    pub fn as_rxs(
        &mut self,
    ) -> impl Iterator<Item = &mut mpsc::UnboundedReceiver<(TestCommand, LogIndex)>> {
        self.nodes.values_mut().map(|node| &mut node.as_rx)
    }

    pub fn is_finished(&self) -> bool {
        self.nodes
            .values()
            .all(|node| node.task_manager.is_finished())
    }

    pub async fn wait_for_node_shutdown(&self, node_id: u64, duration: Duration) {
        let node = self
            .nodes
            .get(&node_id)
            .expect("{node_id} should exist in nodes");
        let res = std::iter::once(node);
        timeout(duration, Self::wait_for_targets_shutdown(res))
            .await
            .expect("wait for group to shutdown timeout");
        assert!(
            node.task_manager.is_finished(),
            "The target node({node_id}) is not finished yet"
        );
    }

    pub async fn wait_for_group_shutdown(&self, duration: Duration) {
        timeout(
            duration,
            Self::wait_for_targets_shutdown(self.nodes.values()),
        )
        .await
        .expect("wait for group to shutdown timeout");
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(self.is_finished(), "The group is not finished yet");
    }

    async fn wait_for_targets_shutdown(targets: impl Iterator<Item = &CurpNode>) {
        let listeners = targets
            .flat_map(|node| {
                BOTTOM_TASKS
                    .iter()
                    .map(|task| {
                        node.task_manager
                            .get_shutdown_listener(task.to_owned())
                            .unwrap()
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let waiters: Vec<_> = listeners.iter().map(|l| l.wait()).collect();
        futures::future::join_all(waiters.into_iter()).await;
    }

    async fn stop(&mut self) {
        debug!("curp group stopping");

        let futs = self
            .nodes
            .values_mut()
            .map(|n| n.task_manager.shutdown(true));
        futures::future::join_all(futs).await;

        self.nodes.clear();
        self.listeners.shutdown();
        self._accept_handle.abort();
        debug!("curp group stopped");

        if let Some(ref path) = self.storage_path {
            _ = std::fs::remove_dir_all(path);
        }
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

    pub async fn get_leader(&self) -> (ServerId, u64) {
        for _ in 0..30 {
            if let Some(leader) = self.try_get_leader().await {
                return leader;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        panic!("can't get leader");
    }

    /// Fetch cluster info via QUIC for a new node joining the cluster
    pub async fn fetch_cluster_info(&self, addrs: &[String], name: &str) -> ClusterInfo {
        let leader_id = self.get_leader().await.0;
        let node = &self.nodes[&leader_id];
        let channel =
            QuicChannel::connect_single_for_test(&node.addr, Arc::clone(&self.quic_client))
                .await
                .expect("connect to leader");

        let cluster_res: FetchClusterResponse = channel
            .unary_call(
                MethodId::FetchCluster,
                FetchClusterRequest {
                    linearizable: false,
                },
                vec![],
                Duration::from_secs(5),
            )
            .await
            .expect("fetch cluster");

        let client_urls: Vec<String> = vec![];
        ClusterInfo::from_cluster(cluster_res, addrs, client_urls.as_slice(), name)
    }

    /// Fetch cluster from a specific node via QUIC
    pub async fn fetch_cluster_from_node(&self, node_id: &ServerId) -> FetchClusterResponse {
        let node = &self.nodes[node_id];
        let channel =
            QuicChannel::connect_single_for_test(&node.addr, Arc::clone(&self.quic_client))
                .await
                .expect("connect to node");

        channel
            .unary_call(
                MethodId::FetchCluster,
                FetchClusterRequest {
                    linearizable: false,
                },
                vec![],
                Duration::from_secs(5),
            )
            .await
            .expect("fetch cluster from node")
    }
}

impl Drop for CurpGroup {
    fn drop(&mut self) {
        block_in_place(move || {
            Handle::current().block_on(async move {
                self.stop().await;
            });
        });
    }
}
