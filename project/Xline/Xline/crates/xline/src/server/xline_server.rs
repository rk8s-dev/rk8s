use std::{sync::Arc, time::Duration};

use tokio::fs;

use anyhow::{Result, anyhow};
use clippy_utilities::{NumericCast, OverflowArithmetic};
use curp::{
    self,
    client::ClientBuilder as CurpClientBuilder,
    members::{ClusterInfo, get_cluster_info_from_remote},
    rpc::{QuicGrpcServer, TransportConfig},
    server::{DB as CurpDB, Rpc, StorageApi as _},
};
use dashmap::DashMap;
use engine::{MemorySnapshotAllocator, RocksSnapshotAllocator, SnapshotAllocator};
use jsonwebtoken::{DecodingKey, EncodingKey};
use tonic::transport::{Certificate, ClientTlsConfig, Identity, ServerTlsConfig};
use tracing::{info, warn};

use utils::{
    barrier::IdBarrier,
    config::{
        AuthConfig, ClusterConfig, CompactConfig, EngineConfig, InitialClusterState, StorageConfig,
        TlsConfig,
    },
    task_manager::{TaskManager, tasks::TaskName},
};
use xlineapi::command::{Command, CurpClient};

use super::{
    auth_server::{AuthServer, Server as AuthEndpointServer},
    auth_wrapper::{AuthWrapper, Server as AuthWrapperEndpointServer},
    cluster_server::{ClusterServer, Server as ClusterEndpointServer},
    command::{Alarmer, CommandExecutor},
    kv_server::{KvServer, Server as KvEndpointServer},
    lease_server::{LeaseServer, Server as LeaseEndpointServer},
    lock_server::{LockServer, Server as LockEndPointServer},
    maintenance::{MaintenanceServer, Server as MaintenanceEndpointServer},
    quic_service::XlineQuicService,
    watch_server::{CHANNEL_SIZE, Server as WatchEndpointServer, WatchServer},
};
use crate::{
    conflict::{XlineSpeculativePools, XlineUncommittedPools},
    header_gen::HeaderGenerator,
    id_gen::IdGenerator,
    metrics::Metrics,
    router::{RouterBuilder, Server},
    state::State,
    storage::{
        AlarmStore, AuthStore, KvStore, LeaseStore,
        compact::{COMPACT_CHANNEL_SIZE, auto_compactor, compact_bg_task},
        db::DB,
        index::Index,
        kv_store::KvStoreInner,
        kvwatcher::KvWatcher,
        lease_store::LeaseCollection,
    },
};

/// Rpc Server of curp protocol
pub(crate) type CurpServer = Rpc<Command, CommandExecutor, State<Arc<CurpClient>>>;

/// Xline server
#[allow(unused)]
pub struct XlineServer {
    /// Cluster information
    cluster_info: Arc<ClusterInfo>,
    /// Cluster Config
    cluster_config: ClusterConfig,
    /// Storage config,
    storage_config: StorageConfig,
    /// Compact config
    compact_config: CompactConfig,
    /// Auth config
    auth_config: AuthConfig,
    /// Client tls config
    client_tls_config: Option<ClientTlsConfig>,
    /// Server tls config
    _server_tls_config: Option<ServerTlsConfig>,
    /// QUIC client for curp peer communication
    quic_client: Arc<gm_quic::prelude::QuicClient>,
    /// Peer TLS certificate (DER) for QUIC server, None = self-signed fallback
    peer_cert_der: Option<Vec<u8>>,
    /// Peer TLS private key (DER) for QUIC server
    peer_key_der: Option<Vec<u8>>,
    /// Peer CA certificate (DER) for QUIC server mTLS client verification
    peer_ca_cert_der: Option<Vec<u8>>,
    /// Task Manager
    task_manager: Arc<TaskManager>,
    /// Curp storage
    curp_storage: Arc<CurpDB<Command>>,
    /// TLS Config
    tls_config: TlsConfig,
}

impl std::fmt::Debug for XlineServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XlineServer")
            .field("cluster_info", &self.cluster_info)
            .field("cluster_config", &self.cluster_config)
            .field("storage_config", &self.storage_config)
            .field("compact_config", &self.compact_config)
            .field("auth_config", &self.auth_config)
            .field("quic_client", &"QuicClient(..)")
            .finish()
    }
}

impl XlineServer {
    /// New `XlineServer`
    ///
    /// # Errors
    ///
    /// Return error if init cluster info failed
    #[inline]
    pub async fn new(
        cluster_config: ClusterConfig,
        storage_config: StorageConfig,
        compact_config: CompactConfig,
        auth_config: AuthConfig,
        tls_config: TlsConfig,
    ) -> Result<Self> {
        let (client_tls_config, server_tls_config) = Self::read_tls_config(&tls_config).await?;
        let curp_storage = Arc::new(CurpDB::open(&cluster_config.curp_config().engine_cfg)?);

        // Read peer TLS certs for QUIC server (DER format)
        let (peer_cert_der, peer_key_der) = Self::read_peer_tls_der(&tls_config).await?;

        // Read peer CA cert for QUIC server mTLS
        let peer_ca_cert_der = if let Some(ca_path) = tls_config.peer_ca_cert_path() {
            Some(fs::read(ca_path).await?)
        } else {
            None
        };

        // Build QUIC client for curp peer communication with proper TLS
        let quic_client = Arc::new(Self::build_quic_client(&tls_config).await?);
        let transport = TransportConfig {
            client: Arc::clone(&quic_client),
            dns_fallback: curp::rpc::DnsFallback::Disabled,
        };

        let cluster_info = Arc::new(
            Self::init_cluster_info(&cluster_config, curp_storage.as_ref(), &transport).await?,
        );
        Ok(Self {
            cluster_info,
            cluster_config,
            storage_config,
            compact_config,
            auth_config,
            client_tls_config,
            _server_tls_config: server_tls_config,
            quic_client,
            peer_cert_der,
            peer_key_der,
            peer_ca_cert_der,
            task_manager: Arc::new(TaskManager::new()),
            curp_storage,
            tls_config,
        })
    }

    /// Init cluster info from cluster config
    async fn init_cluster_info(
        cluster_config: &ClusterConfig,
        curp_storage: &CurpDB<Command>,
        transport: &TransportConfig,
    ) -> Result<ClusterInfo> {
        let name = cluster_config.name().clone();
        let all_members = cluster_config.peers().clone();
        let self_client_urls = cluster_config.client_advertise_urls().clone();
        let self_peer_urls = cluster_config.peer_advertise_urls().clone();
        match (
            curp_storage.recover_cluster_info()?,
            *cluster_config.initial_cluster_state(),
        ) {
            (Some(cluster_info), _) => {
                info!("get cluster_info from local");
                Ok(cluster_info)
            }
            (None, InitialClusterState::New) => {
                info!("get cluster_info by args");
                let cluster_info =
                    ClusterInfo::from_members_map(all_members, &self_client_urls, &name);
                curp_storage.put_cluster_info(&cluster_info)?;
                Ok(cluster_info)
            }
            (None, InitialClusterState::Existing) => {
                info!("get cluster_info from remote");
                let cluster_info = get_cluster_info_from_remote(
                    &ClusterInfo::from_members_map(all_members, &self_client_urls, &name),
                    &self_peer_urls,
                    cluster_config.name(),
                    *cluster_config.client_config().wait_synced_timeout(),
                    transport,
                )
                .await
                .ok_or_else(|| anyhow!("Failed to get cluster info from remote"))?;
                curp_storage.put_cluster_info(&cluster_info)?;
                Ok(cluster_info)
            }
            (None, _) => {
                unreachable!("xline only supports two initial cluster states: new, existing")
            }
        }
    }

    /// Construct a `LeaseCollection`
    #[inline]
    #[allow(clippy::arithmetic_side_effects)] // never overflow
    fn construct_lease_collection(
        heartbeat_interval: Duration,
        candidate_timeout_ticks: u8,
    ) -> Arc<LeaseCollection> {
        let min_ttl = 3 * heartbeat_interval * candidate_timeout_ticks.numeric_cast() / 2;
        // Safe ceiling
        let min_ttl_secs = min_ttl
            .as_secs()
            .overflow_add(u64::from(min_ttl.subsec_nanos() > 0));
        Arc::new(LeaseCollection::new(min_ttl_secs.numeric_cast()))
    }

    /// Construct underlying storages, including `KvStore`, `LeaseStore`,
    /// `AuthStore`
    #[allow(clippy::type_complexity)] // it is easy to read
    #[inline]
    async fn construct_underlying_storages(
        &self,
        db: Arc<DB>,
        lease_collection: Arc<LeaseCollection>,
        header_gen: Arc<HeaderGenerator>,
        key_pair: Option<(EncodingKey, DecodingKey)>,
    ) -> Result<(
        Arc<KvStore>,
        Arc<LeaseStore>,
        Arc<AuthStore>,
        Arc<AlarmStore>,
        Arc<KvWatcher>,
    )> {
        let (compact_task_tx, compact_task_rx) = flume::bounded(COMPACT_CHANNEL_SIZE);
        let index = Arc::new(Index::new());
        let (kv_update_tx, kv_update_rx) = flume::bounded(CHANNEL_SIZE);
        let kv_store_inner = Arc::new(KvStoreInner::new(Arc::clone(&index), Arc::clone(&db)));
        let kv_storage = Arc::new(KvStore::new(
            Arc::clone(&kv_store_inner),
            Arc::clone(&header_gen),
            kv_update_tx.clone(),
            compact_task_tx,
            Arc::clone(&lease_collection),
        ));
        self.task_manager.spawn(TaskName::CompactBg, |n| {
            compact_bg_task(
                Arc::clone(&kv_storage),
                index,
                *self.compact_config.compact_batch_size(),
                *self.compact_config.compact_sleep_interval(),
                compact_task_rx,
                n,
            )
        });
        let lease_storage = Arc::new(LeaseStore::new(
            Arc::clone(&lease_collection),
            Arc::clone(&header_gen),
            Arc::clone(&db),
            kv_update_tx,
            *self.cluster_config.is_leader(),
        ));
        let auth_storage = Arc::new(AuthStore::new(
            lease_collection,
            key_pair,
            Arc::clone(&header_gen),
            Arc::clone(&db),
        ));
        let alarm_storage = Arc::new(AlarmStore::new(header_gen, db));

        let watcher = KvWatcher::new_arc(
            kv_store_inner,
            kv_update_rx,
            *self.cluster_config.server_timeout().sync_victims_interval(),
            &self.task_manager,
        );
        // lease storage must recover before kv storage
        lease_storage.recover()?;
        kv_storage.recover().await?;
        auth_storage.recover()?;
        alarm_storage.recover()?;
        Ok((
            kv_storage,
            lease_storage,
            auth_storage,
            alarm_storage,
            watcher,
        ))
    }

    /// Construct a header generator
    #[inline]
    fn construct_generator(cluster_info: &ClusterInfo) -> (Arc<HeaderGenerator>, Arc<IdGenerator>) {
        let member_id = cluster_info.self_id();
        let cluster_id = cluster_info.cluster_id();
        (
            Arc::new(HeaderGenerator::new(cluster_id, member_id)),
            Arc::new(IdGenerator::new(member_id)),
        )
    }

    /// Init xline and curp router
    ///
    /// # Errors
    ///
    /// Will return `Err` when `init_servers` return an error
    #[inline]
    pub(crate) async fn init_routers(
        &self,
        db: Arc<DB>,
        key_pair: Option<(EncodingKey, DecodingKey)>,
    ) -> Result<(
        RouterBuilder,
        Arc<CurpClient>,
        QuicGrpcServer<Command, CommandExecutor, State<Arc<CurpClient>>, XlineQuicService>,
    )> {
        let (
            kv_server,
            lock_server,
            lease_server,
            auth_server,
            watch_server,
            maintenance_server,
            cluster_server,
            curp_server,
            auth_wrapper,
            curp_client,
        ) = self.init_servers(db, key_pair).await?;
        let mut builder = RouterBuilder::new();

        builder = builder.tls_config(&self.tls_config);

        let xline_router = builder
            .clone()
            .add_subrouter(
                "/v3lockpb.Lock",
                LockEndPointServer::new(lock_server).endpoint().into(),
            )
            .add_subrouter(
                "/etcdserverpb.Auth",
                AuthEndpointServer::new(auth_server.clone())
                    .endpoint()
                    .into(),
            )
            .add_subrouter(
                "/etcdserverpb.Lease",
                LeaseEndpointServer::from_arc(lease_server.clone())
                    .endpoint()
                    .into(),
            )
            .add_subrouter(
                "/etcdserverpb.KV",
                KvEndpointServer::new(kv_server.clone()).endpoint().into(),
            )
            .add_subrouter(
                "/etcdserverpb.Watch",
                WatchEndpointServer::new(watch_server.clone())
                    .endpoint()
                    .into(),
            )
            .add_subrouter(
                "/etcdserverpb.Maintenance",
                MaintenanceEndpointServer::new(maintenance_server.clone())
                    .endpoint()
                    .into(),
            )
            .add_subrouter(
                "/etcdserverpb.Cluster",
                ClusterEndpointServer::new(cluster_server.clone())
                    .endpoint()
                    .into(),
            )
            .add_subrouter(
                "/commandpb.Protocol",
                AuthWrapperEndpointServer::new(auth_wrapper.clone())
                    .endpoint()
                    .into(),
            );

        // Curp peer communication uses QUIC, with AuthWrapper for token-based auth
        let quic_server = QuicGrpcServer::new_with_service(auth_wrapper, curp_server)
            .with_extension(XlineQuicService::new(
                auth_server,
                cluster_server,
                kv_server,
                lease_server,
                maintenance_server,
                watch_server,
            ));

        let xline_router = {
            let (mut reporter, health_server) = tonic_health::server::health_reporter();
            reporter
                .set_service_status("", tonic_health::ServingStatus::Serving)
                .await;
            xline_router.add_service("/*path", health_server)
        };

        Ok((xline_router, curp_client, quic_server))
    }

    /// Start `XlineServer` using gm-quic as transport protocol
    ///
    /// # Errors
    ///
    /// Will return `Err` when `tonic::Server` serve return an error
    #[inline]
    pub async fn start_with_quic(&self) -> Result<()> {
        // Start QUIC server for curp peer communication
        let client_listen_urls = self.cluster_config.client_listen_urls().clone();
        let peer_listen_urls = self.cluster_config.peer_listen_urls().clone();

        info!("start xline server on {:?}", client_listen_urls);
        info!("start curp server on {:?}", peer_listen_urls);
        let db = DB::open(&self.storage_config.engine)?;
        let key_pair = Self::read_key_pair(&self.auth_config).await?;
        let (xline_router, curp_client, quic_server) = self.init_routers(db, key_pair).await?;
        let server_name = self.cluster_config.name().clone();
        let server = Server::new_with_grpc_server(
            quic_server,
            client_listen_urls.clone(),
            peer_listen_urls.clone(),
        )
        .map_err(|e| anyhow::anyhow!(e))?
        .add_server(
            &server_name,
            xline_router,
            peer_listen_urls
                .into_iter()
                .chain(client_listen_urls.into_iter()),
        );

        self.task_manager
            .spawn(TaskName::CurpServer, |n| async move {
                tokio::select! {
                    res = server.serve() => {
                        if let Err(e) = res {
                            tracing::error!("{e}");
                        }
                    },
                    _ = n.wait() => {},
                }
            });
        if let Err(e) = self.publish(curp_client).await {
            warn!("publish name to cluster failed: {e:?}");
        }
        Ok(())
    }

    /// Compatibility entrypoint for test utils that previously passed bound listeners.
    ///
    /// The current QUIC server binds from configured URLs, so the listeners are dropped
    /// here to release ports before delegating to `start_with_quic`.
    #[inline]
    pub async fn start_from_listener(
        &self,
        xline_listener: tokio::net::TcpListener,
        curp_listener: tokio::net::TcpListener,
    ) -> Result<()> {
        drop((xline_listener, curp_listener));
        self.start_with_quic().await
    }

    /// Init `KvServer`, `LockServer`, `LeaseServer`, `WatchServer` and
    /// `CurpServer` for the Xline Server.
    #[allow(
        clippy::type_complexity, // it is easy to read
        clippy::too_many_lines, // TODO: split this into multiple functions
        clippy::as_conversions, // cast to dyn
        trivial_casts // same as above
    )]
    async fn init_servers(
        &self,
        db: Arc<DB>,
        key_pair: Option<(EncodingKey, DecodingKey)>,
    ) -> Result<(
        KvServer,
        LockServer,
        Arc<LeaseServer>,
        AuthServer,
        WatchServer,
        MaintenanceServer,
        ClusterServer,
        CurpServer,
        AuthWrapper,
        Arc<CurpClient>,
    )> {
        let (header_gen, id_gen) = Self::construct_generator(&self.cluster_info);
        let lease_collection = Self::construct_lease_collection(
            self.cluster_config.curp_config().heartbeat_interval,
            self.cluster_config.curp_config().candidate_timeout_ticks,
        );

        let (kv_storage, lease_storage, auth_storage, alarm_storage, watcher) = self
            .construct_underlying_storages(
                Arc::clone(&db),
                Arc::clone(&lease_collection),
                Arc::clone(&header_gen),
                key_pair,
            )
            .await?;

        let id_barrier = Arc::new(IdBarrier::new());
        let compact_events = Arc::new(DashMap::new());
        let ce = Arc::new(CommandExecutor::new(
            Arc::clone(&kv_storage),
            Arc::clone(&auth_storage),
            Arc::clone(&lease_storage),
            Arc::clone(&alarm_storage),
            Arc::clone(&db),
            Arc::clone(&id_barrier),
            Arc::clone(&compact_events),
            self.storage_config.quota,
        ));
        let snapshot_allocator: Box<dyn SnapshotAllocator> = match self.storage_config.engine {
            EngineConfig::Memory => Box::<MemorySnapshotAllocator>::default(),
            EngineConfig::RocksDB(_) => Box::<RocksSnapshotAllocator>::default(),
            #[allow(clippy::unimplemented)]
            _ => unimplemented!(),
        };

        let auto_compactor = if let Some(auto_config_cfg) = *self.compact_config.auto_compactor() {
            Some(
                auto_compactor(
                    *self.cluster_config.is_leader(),
                    header_gen.general_revision_arc(),
                    auto_config_cfg,
                    Arc::clone(&self.task_manager),
                )
                .await,
            )
        } else {
            None
        };

        let auto_compactor_c = auto_compactor.clone();

        let state = State::new(Arc::clone(&lease_storage), auto_compactor);

        let curp_config = Arc::new(self.cluster_config.curp_config().clone());

        let curp_server = CurpServer::new(
            Arc::clone(&self.cluster_info),
            *self.cluster_config.is_leader(),
            Arc::clone(&ce),
            snapshot_allocator,
            state,
            Arc::clone(&curp_config),
            Arc::clone(&self.curp_storage),
            Arc::clone(&self.task_manager),
            XlineSpeculativePools::new(Arc::clone(&lease_collection)).into_inner(),
            XlineUncommittedPools::new(lease_collection).into_inner(),
            Arc::clone(&self.quic_client),
        )
        .map_err(|e| anyhow::anyhow!("failed to create curp service: {e:?}"))?;

        let client = Arc::new(
            CurpClientBuilder::new(*self.cluster_config.client_config(), false)
                .quic_transport(Arc::clone(&self.quic_client))
                .cluster_version(self.cluster_info.cluster_version())
                .all_members(self.cluster_info.all_members_peer_urls())
                .bypass(self.cluster_info.self_id(), curp_server.clone())
                .build::<Command>()?,
        ) as Arc<CurpClient>;

        if let Some(compactor) = auto_compactor_c {
            compactor.set_compactable(Arc::clone(&client)).await;
        }
        ce.set_alarmer(Alarmer::new(
            self.cluster_info.self_id(),
            Arc::clone(&client),
        ));
        let raw_curp = curp_server.raw_curp();

        Metrics::register_callback()?;

        let server_timeout = self.cluster_config.server_timeout();
        Ok((
            KvServer::new(
                Arc::clone(&kv_storage),
                Arc::clone(&auth_storage),
                *server_timeout.compact_timeout(),
                Arc::clone(&client),
                compact_events,
            ),
            LockServer::new(
                Arc::clone(&client),
                Arc::clone(&auth_storage),
                Arc::clone(&id_gen),
                &self.cluster_info.self_peer_urls(),
                self.client_tls_config.as_ref(),
            ),
            LeaseServer::new(
                lease_storage,
                Arc::clone(&auth_storage),
                Arc::clone(&client),
                id_gen,
                Arc::clone(&self.cluster_info),
                Arc::clone(&self.quic_client),
                &self.task_manager,
            ),
            AuthServer::new(Arc::clone(&client), Arc::clone(&auth_storage)),
            WatchServer::new(
                watcher,
                Arc::clone(&header_gen),
                *server_timeout.watch_progress_notify_interval(),
                Arc::clone(&self.task_manager),
            ),
            MaintenanceServer::new(
                kv_storage,
                Arc::clone(&auth_storage),
                Arc::clone(&client),
                db,
                Arc::clone(&header_gen),
                Arc::clone(&self.cluster_info),
                raw_curp,
                ce,
                alarm_storage,
            ),
            ClusterServer::new(Arc::clone(&client), header_gen),
            curp_server.clone(),
            AuthWrapper::new(curp_server, auth_storage),
            client,
        ))
    }

    /// Publish the name of current node to cluster
    async fn publish(&self, curp_client: Arc<CurpClient>) -> Result<(), xlinerpc::Status> {
        curp_client
            .propose_publish(
                self.cluster_info.self_id(),
                self.cluster_info.self_name(),
                self.cluster_info.self_client_urls(),
            )
            .await
    }

    /// Stop `XlineServer`
    #[inline]
    pub async fn stop(&self) {
        self.task_manager.shutdown(true).await;
    }

    /// Read key pair from file
    async fn read_key_pair(auth_config: &AuthConfig) -> Result<Option<(EncodingKey, DecodingKey)>> {
        match (
            auth_config.auth_private_key().as_ref(),
            auth_config.auth_public_key().as_ref(),
        ) {
            (Some(private), Some(public)) => {
                let encoding_key = EncodingKey::from_rsa_pem(&fs::read(private).await?)?;
                let decoding_key = DecodingKey::from_rsa_pem(&fs::read(public).await?)?;
                Ok(Some((encoding_key, decoding_key)))
            }
            (None, None) => Ok(None),
            _ => Err(anyhow!(
                "private key path and public key path must be both set or both unset"
            )),
        }
    }

    /// Read tls cert and key from file
    async fn read_tls_config(
        tls_config: &TlsConfig,
    ) -> Result<(Option<ClientTlsConfig>, Option<ServerTlsConfig>)> {
        let client_tls_config = match (
            tls_config.client_ca_cert_path().as_ref(),
            tls_config.client_cert_path().as_ref(),
            tls_config.client_key_path().as_ref(),
        ) {
            (Some(ca_path), Some(cert_path), Some(key_path)) => {
                let ca = fs::read(ca_path).await?;
                let cert = fs::read(cert_path).await?;
                let key = fs::read(key_path).await?;
                Some(
                    ClientTlsConfig::new()
                        .ca_certificate(Certificate::from_pem(ca))
                        .identity(Identity::from_pem(cert, key)),
                )
            }
            (Some(ca_path), None, None) => {
                let ca = fs::read(ca_path).await?;
                Some(ClientTlsConfig::new().ca_certificate(Certificate::from_pem(ca)))
            }
            (_, Some(_), None) | (_, None, Some(_)) => {
                return Err(anyhow!(
                    "client_cert_path and client_key_path must be both set"
                ));
            }
            _ => None,
        };
        let server_tls_config = match (
            tls_config.peer_ca_cert_path().as_ref(),
            tls_config.peer_cert_path().as_ref(),
            tls_config.peer_key_path().as_ref(),
        ) {
            (Some(ca_path), Some(cert_path), Some(key_path)) => {
                let ca = fs::read(ca_path).await?;
                let cert = fs::read_to_string(cert_path).await?;
                let key = fs::read_to_string(key_path).await?;
                Some(
                    ServerTlsConfig::new()
                        .client_ca_root(Certificate::from_pem(ca))
                        .identity(Identity::from_pem(cert, key)),
                )
            }
            (None, Some(cert_path), Some(key_path)) => {
                let cert = fs::read_to_string(cert_path).await?;
                let key = fs::read_to_string(key_path).await?;
                Some(ServerTlsConfig::new().identity(Identity::from_pem(cert, key)))
            }
            (_, Some(_), None) | (_, None, Some(_)) => {
                return Err(anyhow!("peer_cert_path and peer_key_path must be both set"));
            }
            _ => None,
        };
        Ok((client_tls_config, server_tls_config))
    }

    /// Build a QUIC client with proper TLS configuration from `TlsConfig`.
    ///
    /// If `peer_ca_cert_path` is configured, the CA cert is loaded into the root
    /// store so the client verifies server identity. If `peer_cert_path` and
    /// `peer_key_path` are also set, mTLS client authentication is enabled.
    /// Without any peer TLS config, the client uses an empty trust store (no
    /// verification) and logs a warning.
    async fn build_quic_client(tls_config: &TlsConfig) -> Result<gm_quic::prelude::QuicClient> {
        let mut root_store = rustls::RootCertStore::empty();

        if let Some(ca_path) = tls_config.peer_ca_cert_path() {
            let ca_pem = fs::read(ca_path).await?;
            let certs: Vec<_> =
                rustls_pemfile::certs(&mut &ca_pem[..]).collect::<Result<Vec<_>, _>>()?;
            for cert in certs {
                root_store
                    .add(cert)
                    .map_err(|e| anyhow!("failed to add peer CA cert: {e}"))?;
            }
        } else {
            warn!(
                "No peer CA certificate configured; QUIC peer connections will not verify server identity"
            );
        }

        let builder = gm_quic::prelude::QuicClient::builder().with_root_certificates(root_store);

        let client = match (tls_config.peer_cert_path(), tls_config.peer_key_path()) {
            (Some(cert_path), Some(key_path)) => {
                // gm-quic's with_cert accepts &Path directly (handles PEM/DER parsing)
                builder
                    .with_cert(cert_path.as_path(), key_path.as_path())
                    .build()
            }
            _ => builder.without_cert().build(),
        };

        Ok(client)
    }

    /// Read peer TLS certificate and private key files as raw bytes for the
    /// QUIC server. Returns `(None, None)` if not configured. The raw bytes
    /// (PEM or DER) are passed directly to gm-quic which handles parsing.
    async fn read_peer_tls_der(
        tls_config: &TlsConfig,
    ) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>)> {
        match (tls_config.peer_cert_path(), tls_config.peer_key_path()) {
            (Some(cert_path), Some(key_path)) => {
                let cert_bytes = fs::read(cert_path).await?;
                let key_bytes = fs::read(key_path).await?;
                Ok((Some(cert_bytes), Some(key_bytes)))
            }
            (Some(_), None) | (None, Some(_)) => {
                Err(anyhow!("peer_cert_path and peer_key_path must be both set"))
            }
            _ => Ok((None, None)),
        }
    }
}
