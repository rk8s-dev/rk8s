use std::{collections::HashMap, env::temp_dir, iter, path::PathBuf, sync::Arc};

use futures::future::join_all;
use rand::{Rng, distributions::Alphanumeric, thread_rng};
use tokio::{
    net::TcpListener,
    runtime::Handle,
    task::block_in_place,
    time::{self, Duration},
};
use utils::config::{
    AuthConfig, ClusterConfig, CompactConfig, EngineConfig, InitialClusterState, LogConfig,
    MetricsConfig, StorageConfig, TlsConfig, TraceConfig, XlineServerConfig, default_quota,
};
use xline::server::XlineServer;
use xline_client::types::{auth::PermissionType, range_end::RangeOption};
pub use xline_client::{Client, ClientOptions, clients, types};
use xlinerpc::QuicTlsConfig;

#[inline]
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// Get server-specific certificate path based on server index
fn server_cert_path(server_idx: usize) -> PathBuf {
    let server_name = format!("server{}", server_idx);
    fixture_path(&format!("{}.crt", server_name))
}

fn server_key_path(server_idx: usize) -> PathBuf {
    let server_name = format!("server{}", server_idx);
    fixture_path(&format!("{}.key", server_name))
}

#[inline]
fn default_quic_tls_config_for_tests() -> TlsConfig {
    TlsConfig::new(
        None,
        Some(fixture_path("server.crt")),
        Some(fixture_path("server.key")),
        Some(fixture_path("ca.crt")),
        None,
        None,
    )
}

/// Create QUIC TLS config for a specific server with its own certificate
fn server_quic_tls_config_for_tests(server_idx: usize) -> TlsConfig {
    let cert_path = server_cert_path(server_idx);
    let key_path = server_key_path(server_idx);

    // If server-specific certs don't exist, fall back to default
    if cert_path.exists() && key_path.exists() {
        TlsConfig::new(
            None,
            Some(cert_path),
            Some(key_path),
            Some(fixture_path("ca.crt")),
            None,
            None,
        )
    } else {
        default_quic_tls_config_for_tests()
    }
}

#[inline]
fn ensure_quic_tls(config: XlineServerConfig, server_idx: usize) -> XlineServerConfig {
    if config.tls().server_tls_enabled() {
        return config;
    }

    // Always use per-server TLS certificates so that the server name (e.g., "server0")
    // matches the certificate's SAN. The default server.crt only has "localhost" in SAN,
    // which won't match the DNS-based server names required for QUIC SNI.
    let tls_config = server_quic_tls_config_for_tests(server_idx);

    XlineServerConfig::new(
        config.cluster().clone(),
        config.storage().clone(),
        config.log().clone(),
        config.trace().clone(),
        config.auth().clone(),
        *config.compact(),
        tls_config,
        config.metrics().clone(),
    )
}

#[inline]
fn normalize_connect_addr(addr: std::net::SocketAddr) -> std::net::SocketAddr {
    match addr {
        std::net::SocketAddr::V4(v4) => {
            let ip = if v4.ip().is_unspecified() {
                std::net::Ipv4Addr::LOCALHOST
            } else {
                *v4.ip()
            };
            std::net::SocketAddr::new(ip.into(), v4.port())
        }
        std::net::SocketAddr::V6(v6) => {
            let ip = if v6.ip().is_unspecified() {
                std::net::Ipv6Addr::LOCALHOST
            } else {
                *v6.ip()
            };
            std::net::SocketAddr::new(ip.into(), v6.port())
        }
    }
}

/// Cluster
pub struct Cluster {
    /// client and peer listeners of members
    listeners: Vec<(TcpListener, TcpListener)>,
    /// address of members
    all_members_peer_urls: Vec<String>,
    /// address of members
    all_members_client_urls: Vec<String>,
    /// Server configs
    configs: Vec<XlineServerConfig>,
    /// Xline servers
    servers: Vec<Arc<XlineServer>>,
    /// Client of cluster
    client: Option<Client>,
}

impl Cluster {
    /// New `Cluster`
    pub async fn new(size: usize) -> Self {
        let configs = iter::repeat_with(|| XlineServerConfig::default())
            .enumerate()
            .map(|(i, c)| ensure_quic_tls(c, i))
            .take(size)
            .collect();
        Self::new_with_configs(configs).await
    }

    /// New `Cluster` with rocksdb
    pub async fn new_rocks(size: usize) -> Self {
        let configs = iter::repeat_with(|| {
            let path = temp_dir().join(random_id());
            Self::default_rocks_config_with_path(path)
        })
        .enumerate()
        .map(|(i, c)| ensure_quic_tls(c, i))
        .take(size)
        .collect();
        Self::new_with_configs(configs).await
    }

    pub async fn new_with_configs(configs: Vec<XlineServerConfig>) -> Self {
        let configs = configs
            .into_iter()
            .enumerate()
            .map(|(i, c)| ensure_quic_tls(c, i))
            .collect::<Vec<_>>();
        let size = configs.len();
        let mut listeners = Vec::new();
        for _i in 0..size {
            // Bind to 127.0.0.1 for all servers
            let bind_addr = "127.0.0.1".to_string();

            let client_addr = format!("{}:0", bind_addr);
            let peer_addr = format!("{}:0", bind_addr);

            listeners.push((
                TcpListener::bind(&client_addr).await.unwrap(),
                TcpListener::bind(&peer_addr).await.unwrap(),
            ));
        }
        let server_tls_enabled = configs.iter().any(|c| c.tls().server_tls_enabled());
        let scheme = if server_tls_enabled { "https" } else { "http" };
        let all_members_client_urls = listeners
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let port = l.0.local_addr().unwrap().port();
                // Use DNS name for SNI matching - servers are registered with DNS names
                format!("{scheme}://server{i}:{port}")
            })
            .collect();
        let all_members_peer_urls = listeners
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let port = l.1.local_addr().unwrap().port();
                format!("{scheme}://server{i}:{port}")
            })
            .collect();
        Self {
            listeners,
            all_members_peer_urls,
            all_members_client_urls,
            configs,
            servers: Vec::new(),
            client: None,
        }
    }

    /// Start `Cluster`
    pub async fn start(&mut self) {
        let mut futs = Vec::new();
        for (i, config) in self.configs.iter().enumerate() {
            // Always use DNS name as server name (e.g., "server0") for TLS SNI matching.
            // IP addresses cannot be used as SNI per RFC 6066, so the server must
            // be registered with a DNS name that the certificate's SAN includes.
            let name = format!("server{}", i);
            let (xline_listener, curp_listener) = self.listeners.remove(0);
            let scheme = if config.tls().server_tls_enabled() {
                "https"
            } else {
                "http"
            };
            let self_client_url = self.get_client_url(i);
            let self_peer_url = self.get_peer_url(i);
            let self_client_listen_url =
                format!("{scheme}://{}", xline_listener.local_addr().unwrap());
            let self_peer_listen_url =
                format!("{scheme}://{}", curp_listener.local_addr().unwrap());

            let config = Self::merge_config(
                config,
                name.clone(),
                self_client_listen_url,
                self_client_url,
                self_peer_listen_url,
                self_peer_url,
                self.all_members_peer_urls
                    .clone()
                    .into_iter()
                    .enumerate()
                    .map(|(j, addr)| {
                        let member_name = format!("server{}", j);
                        (member_name, vec![addr])
                    })
                    .collect(),
                i == 0,
                InitialClusterState::New,
            );

            let server = Arc::new(
                XlineServer::new(
                    config.cluster().clone(),
                    config.storage().clone(),
                    *config.compact(),
                    config.auth().clone(),
                    config.tls().clone(),
                )
                .await
                .unwrap(),
            );
            self.servers.push(Arc::clone(&server));

            futs.push(async move {
                let result = server
                    .start_from_listener(xline_listener, curp_listener)
                    .await;
                if let Err(e) = result {
                    panic!("Server start error: {e}");
                }
            });
        }
        join_all(futs).await;
        // Wait for async server tasks to finish binding and accepting connections.
        time::sleep(Duration::from_millis(1500)).await;
    }

    pub async fn run_node(&mut self, xline_listener: TcpListener, curp_listener: TcpListener) {
        let config = XlineServerConfig::default();
        self.run_node_with_config(xline_listener, curp_listener, config)
            .await;
    }

    pub async fn run_node_with_config(
        &mut self,
        xline_listener: TcpListener,
        curp_listener: TcpListener,
        base_config: XlineServerConfig,
    ) {
        let idx = self.all_members_peer_urls.len();
        let name = format!("server{}", idx);
        // Ensure TLS is configured for the new node so it can communicate with existing cluster
        let tls_config = base_config.tls();
        let server_tls_enabled = tls_config.server_tls_enabled();
        let tls_config = if server_tls_enabled {
            tls_config.clone()
        } else {
            // Use per-server TLS certificates so that the server name (e.g., "server3")
            // matches the certificate's SAN
            server_quic_tls_config_for_tests(idx)
        };
        let scheme = if server_tls_enabled { "https" } else { "http" };
        let self_client_listen_url = format!("{scheme}://{}", xline_listener.local_addr().unwrap());
        let self_peer_listen_url = format!("{scheme}://{}", curp_listener.local_addr().unwrap());
        let self_client_url = format!(
            "{scheme}://{}",
            normalize_connect_addr(xline_listener.local_addr().unwrap())
        );
        let self_peer_url = format!(
            "{scheme}://{}",
            normalize_connect_addr(curp_listener.local_addr().unwrap())
        );
        self.all_members_client_urls.push(self_client_url.clone());
        self.all_members_peer_urls.push(self_peer_url.clone());

        let peers = self
            .all_members_peer_urls
            .clone()
            .into_iter()
            .enumerate()
            .map(|(id, addr)| (format!("server{id}"), vec![addr]))
            .collect::<HashMap<_, _>>();
        self.configs.push(base_config);

        let base_config = self.configs.last().unwrap();
        let config = Self::merge_config(
            base_config,
            name,
            self_client_listen_url,
            self_client_url,
            self_peer_listen_url,
            self_peer_url,
            peers,
            false,
            InitialClusterState::Existing,
        );

        // Override TLS config to ensure it matches the cluster
        let config = XlineServerConfig::new(
            config.cluster().clone(),
            config.storage().clone(),
            config.log().clone(),
            config.trace().clone(),
            config.auth().clone(),
            *config.compact(),
            tls_config,
            config.metrics().clone(),
        );

        let server = XlineServer::new(
            config.cluster().clone(),
            config.storage().clone(),
            *config.compact(),
            config.auth().clone(),
            config.tls().clone(),
        )
        .await
        .unwrap();
        let result = server
            .start_from_listener(xline_listener, curp_listener)
            .await;
        if let Err(e) = result {
            panic!("Server start error: {e}");
        }
    }

    /// Create or get the client with the specified index
    pub async fn client(&mut self) -> &mut Client {
        if self.client.is_none() {
            // Use include_bytes! to embed CA certificate at compile time
            // This avoids runtime file I/O and file descriptor leaks
            let ca_cert_bytes = include_bytes!("../../../fixtures/ca.crt");

            // Configure QUIC TLS with CA certificate for server verification
            let quic_tls_config =
                QuicTlsConfig::default().with_peer_ca_cert_pem(ca_cert_bytes.to_vec());

            let options = ClientOptions::default().with_quic_tls_config(quic_tls_config);

            let client = Client::connect(self.all_members_client_urls.clone(), options)
                .await
                .unwrap_or_else(|e| {
                    panic!("Client connect error: {:?}", e);
                });
            self.client = Some(client);
        }
        self.client.as_mut().unwrap()
    }

    /// Create or get the client with the specified index
    pub async fn client_with_quic_tls_config(&mut self, quic_tls_config: QuicTlsConfig) -> Client {
        let opts = ClientOptions::default().with_quic_tls_config(quic_tls_config);
        Client::connect(self.all_members_client_urls.clone(), opts)
            .await
            .unwrap_or_else(|e| {
                panic!("Client connect error: {:?}", e);
            })
    }

    pub fn all_members_client_urls_map(&self) -> HashMap<usize, String> {
        self.all_members_client_urls
            .iter()
            .cloned()
            .enumerate()
            .collect()
    }

    pub fn get_client_url(&self, idx: usize) -> String {
        self.all_members_client_urls[idx].clone()
    }

    pub fn get_peer_url(&self, idx: usize) -> String {
        self.all_members_peer_urls[idx].clone()
    }

    /// Create a QuicTlsConfig for test clients (with CA cert for server verification)
    pub fn create_quic_tls_config() -> xlinerpc::QuicTlsConfig {
        let ca_cert_bytes = include_bytes!("../../../fixtures/ca.crt");
        xlinerpc::QuicTlsConfig::default().with_peer_ca_cert_pem(ca_cert_bytes.to_vec())
    }

    pub fn all_client_addrs(&self) -> Vec<String> {
        self.all_members_client_urls.clone()
    }

    pub fn default_config_with_quota_and_rocks_path(
        path: PathBuf,
        quota: u64,
    ) -> XlineServerConfig {
        let cluster = ClusterConfig::default();
        let storage = StorageConfig::new(EngineConfig::RocksDB(path), quota);
        let log = LogConfig::default();
        let trace = TraceConfig::default();
        let auth = AuthConfig::default();
        let compact = CompactConfig::default();
        let tls = TlsConfig::default();
        let metrics = MetricsConfig::default();
        XlineServerConfig::new(cluster, storage, log, trace, auth, compact, tls, metrics)
    }

    pub fn default_rocks_config_with_path(path: PathBuf) -> XlineServerConfig {
        Self::default_config_with_quota_and_rocks_path(path, default_quota())
    }

    pub fn default_rocks_config() -> XlineServerConfig {
        let path = temp_dir().join(random_id());
        Self::default_config_with_quota_and_rocks_path(path, default_quota())
    }

    pub fn default_quota_config(quota: u64) -> XlineServerConfig {
        let path = temp_dir().join(random_id());
        Self::default_config_with_quota_and_rocks_path(path, quota)
    }

    fn merge_config(
        base_config: &XlineServerConfig,
        name: String,
        client_listen_url: String,
        client_advertise_url: String,
        peer_listen_url: String,
        peer_advertise_url: String,
        peers: HashMap<String, Vec<String>>,
        is_leader: bool,
        initial_cluster_state: InitialClusterState,
    ) -> XlineServerConfig {
        let old_cluster = base_config.cluster();
        let new_cluster = ClusterConfig::new(
            name,
            vec![peer_listen_url],
            vec![peer_advertise_url],
            vec![client_listen_url],
            vec![client_advertise_url],
            peers,
            is_leader,
            old_cluster.curp_config().clone(),
            *old_cluster.client_config(),
            *old_cluster.server_timeout(),
            initial_cluster_state,
        );
        XlineServerConfig::new(
            new_cluster,
            base_config.storage().clone(),
            base_config.log().clone(),
            base_config.trace().clone(),
            base_config.auth().clone(),
            *base_config.compact(),
            base_config.tls().clone(),
            base_config.metrics().clone(),
        )
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        block_in_place(move || {
            Handle::current().block_on(async move {
                let mut handles = Vec::new();
                for xline in self.servers.drain(..) {
                    handles.push(tokio::spawn(async move {
                        xline.stop().await;
                    }));
                }
                for h in handles {
                    h.await.unwrap();
                }
                for cfg in &self.configs {
                    if let EngineConfig::RocksDB(ref path) = cfg.cluster().curp_config().engine_cfg
                    {
                        let _ignore = tokio::fs::remove_dir_all(path).await;
                    }
                    if let EngineConfig::RocksDB(ref path) = cfg.storage().engine {
                        let _ignore = tokio::fs::remove_dir_all(path).await;
                    }
                }
                // Reset the shared QUIC listeners so the next test
                // can create a fresh QuicListeners instance.
                xline::reset_shared_quic();
            });
        });
    }
}

fn random_id() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect()
}

pub async fn set_user(
    client: &Client,
    name: &str,
    password: &str,
    role: &str,
    key: &[u8],
    range_end: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let client = client.auth_client();
    client.user_add(name, password, false).await?;
    client.role_add(role).await?;
    client.user_grant_role(name, role).await?;
    if !key.is_empty() {
        client
            .role_grant_permission(
                role,
                PermissionType::Readwrite,
                key,
                Some(RangeOption::RangeEnd(range_end.to_vec())),
            )
            .await?;
    }
    Ok(())
}

pub async fn enable_auth(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    set_user(client, "root", "123", "root", &[], &[]).await?;
    client.auth_client().auth_enable().await?;
    Ok(())
}
