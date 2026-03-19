use std::{sync::Arc, time::Duration};

use tokio::fs;

use anyhow::{Result, anyhow};
use clippy_utilities::{NumericCast, OverflowArithmetic};
use curp::{
    self,
    client::ClientBuilder as CurpClientBuilder,
    members::{ClusterInfo, get_cluster_info_from_remote},
    server::{DB as CurpDB, Rpc, StorageApi as _},
};
use dashmap::DashMap;
use engine::{MemorySnapshotAllocator, RocksSnapshotAllocator, SnapshotAllocator};
use jsonwebtoken::{DecodingKey, EncodingKey};
use tonic::{
    Status,
    // TODO: use our own status type
    // use xlinerpc::status::Status;
    transport::{
        Certificate, ClientTlsConfig, Identity, ServerTlsConfig
    },
};
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
    auth_server::{
        AuthServer,
        Server as AuthEndpointServer,
    },
    auth_wrapper::{
        Server as AuthWrapperEndpointServer,
        AuthWrapper,
    },
    cluster_server::{
        ClusterServer,
        Server as ClusterEndpointServer,
    },
    command::{Alarmer, CommandExecutor},
    kv_server::{
        KvServer,
        Server as KvEndpointServer,
    },
    lease_server::{
        LeaseServer,
        Server as LeaseEndpointServer,
    },
    lock_server::{LockServer, Server as LockEndPointServer},
    maintenance::{
        MaintenanceServer,
        Server as MaintenanceEndpointServer,
    },
    watch_server::{CHANNEL_SIZE, WatchServer, Server as WatchEndpointServer},
    curp_server::Server as ProtocolEndpointServer,
};
use crate::{
    conflict::{XlineSpeculativePools, XlineUncommittedPools},
    header_gen::HeaderGenerator,
    id_gen::IdGenerator,
    metrics::Metrics,
    router::{
        RouterBuilder,
        Server,
    },
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
#[derive(Debug)]
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
    /// Task Manager
    task_manager: Arc<TaskManager>,
    /// Curp storage
    curp_storage: Arc<CurpDB<Command>>,
    /// TLS Config
    tls_config: TlsConfig,
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
        let cluster_info = Arc::new(
            Self::init_cluster_info(
                &cluster_config,
                curp_storage.as_ref(),
                client_tls_config.as_ref(),
            )
            .await?,
        );
        Ok(Self {
            cluster_info,
            cluster_config,
            storage_config,
            compact_config,
            auth_config,
            client_tls_config,
            _server_tls_config: server_tls_config,
            task_manager: Arc::new(TaskManager::new()),
            curp_storage,
            tls_config,
        })
    }

    /// Init cluster info from cluster config
    async fn init_cluster_info(
        cluster_config: &ClusterConfig,
        curp_storage: &CurpDB<Command>,
        tls_config: Option<&ClientTlsConfig>,
    ) -> Result<ClusterInfo> {
        info!("name = {:?}", cluster_config.name());
        info!("cluster_peers = {:?}", cluster_config.peers());

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
                    tls_config,
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
    pub async fn init_routers(
        &self,
        db: Arc<DB>,
        key_pair: Option<(EncodingKey, DecodingKey)>,
    ) -> Result<(RouterBuilder, RouterBuilder, Arc<CurpClient>)> {
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
        .add_subrouter( "/v3lockpb.Lock", LockEndPointServer::new(lock_server).endpoint().into())
        .add_subrouter( "/etcdserverpb.Auth", AuthEndpointServer::new(auth_server).endpoint().into())
        .add_subrouter( "/etcdserverpb.Lease", LeaseEndpointServer::from_arc(lease_server).endpoint().into())
        .add_subrouter( "/etcdserverpb.KV", KvEndpointServer::new(kv_server).endpoint().into())
        .add_subrouter( "/etcdserverpb.Watch", WatchEndpointServer::new(watch_server).endpoint().into())
        .add_subrouter( "/etcdserverpb.Maintenance", MaintenanceEndpointServer::new(maintenance_server).endpoint().into())
        .add_subrouter( "/etcdserverpb.Cluster", ClusterEndpointServer::new(cluster_server).endpoint().into())
        .add_subrouter( "/commandpb.Protocol", AuthWrapperEndpointServer::new(auth_wrapper).endpoint().into());
        let curp_router = builder
            .add_subrouter("/commandpb.Protocol", ProtocolEndpointServer::new(curp_server.clone()).endpoint().into())
            .add_subrouter("/inner_messagepb.InnerProtocol", ProtocolEndpointServer::new(curp_server).endpoint().into());

        let xline_router = {
            let (mut reporter, health_server) = tonic_health::server::health_reporter();
            reporter
                .set_service_status("", tonic_health::ServingStatus::Serving)
                .await;
            xline_router.add_service("/", health_server)
        };

        Ok((xline_router, curp_router, curp_client))
    }

    /// Start `XlineServer` using gm-quic as transport protocol
    ///
    /// # Errors
    ///
    /// Will return `Err` when `tonic::Server` serve return an error
    #[inline]
    pub async fn start_with_quic(&self) -> Result<()> {
        // parse peer_listen_urls to listen
        let client_listen_urls = self.cluster_config.client_listen_urls().clone();
        let peer_listen_urls = self.cluster_config.peer_listen_urls().clone();
        info!("start xline server on {:?}", client_listen_urls);
        info!("start curp server on {:?}", peer_listen_urls);
        let db = DB::open(&self.storage_config.engine)?;
        let key_pair = Self::read_key_pair(&self.auth_config).await?;
        let (xline_router, curp_router, curp_client) = self.init_routers(db, key_pair).await?;
        let server = Server::new()
            .add_server("localhost", xline_router, client_listen_urls)
            .add_server("127.0.0.1", curp_router, peer_listen_urls);
        self.task_manager
            .spawn(TaskName::TonicServer, |n| async move {
                tokio::select! {
                    _ = server.serve() => {},
                    _ = n.wait() => {},
                }
            });
        if let Err(e) = self.publish(curp_client).await {
            warn!("publish name to cluster failed: {e:?}");
        }
        Ok(())
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
            self.client_tls_config.clone(),
            XlineSpeculativePools::new(Arc::clone(&lease_collection)).into_inner(),
            XlineUncommittedPools::new(lease_collection).into_inner(),
        );

        let client = Arc::new(
            CurpClientBuilder::new(*self.cluster_config.client_config(), false)
                .tls_config(self.client_tls_config.clone())
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
                self.client_tls_config.clone(),
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
    async fn publish(&self, curp_client: Arc<CurpClient>) -> Result<(), Status> {
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
}
