use std::{fmt::Debug, sync::Arc};

use engine::SnapshotAllocator;
use flume::r#async::RecvStream;
use tokio::sync::broadcast;
use tonic::Status;
use tonic::transport::ClientTlsConfig;
use tracing::instrument;
use utils::{config::CurpConfig, task_manager::TaskManager, tracing::Extract};
// TODO: use our own status type
// use xlinerpc::status::Status;
use self::curp_node::CurpNode;
pub use self::{
    conflict::{spec_pool_new::SpObject, uncommitted_pool::UcpObject},
    raw_curp::RawCurp,
};
use crate::rpc::{OpResponse, RecordRequest, RecordResponse};
use crate::{
    cmd::{Command, CommandExecutor},
    members::{ClusterInfo, ServerId},
    role_change::RoleChange,
    rpc::{
        AppendEntriesRequest, AppendEntriesResponse, FetchClusterRequest, FetchClusterResponse,
        FetchReadStateRequest, FetchReadStateResponse, InstallSnapshotRequest,
        InstallSnapshotResponse, LeaseKeepAliveMsg, MoveLeaderRequest, MoveLeaderResponse,
        ProposeConfChangeRequest, ProposeConfChangeResponse, ProposeRequest, PublishRequest,
        PublishResponse, ShutdownRequest, ShutdownResponse, TriggerShutdownRequest,
        TriggerShutdownResponse, TryBecomeLeaderNowRequest, TryBecomeLeaderNowResponse,
        VoteRequest, VoteResponse, connect::Bypass,
    },
};
use crate::{
    response::ResponseSender,
    rpc::{ReadIndexRequest, ReadIndexResponse},
};

/// Command worker to do execution and after sync
mod cmd_worker;

/// Raw Curp
mod raw_curp;

/// Command board is the buffer to store command execution result
mod cmd_board;

/// Conflict pools
pub mod conflict;

/// Background garbage collection for Curp server
mod gc;

/// Curp Node
mod curp_node;

/// Storage
mod storage;

/// Lease Manager
mod lease_manager;

/// Curp metrics
mod metrics;

pub use storage::{StorageApi, StorageError, db::DB};

/// The Rpc Server to handle rpc requests
///
/// This Wrapper is introduced due to the `MadSim` rpc lib
#[derive(Debug)]
pub struct Rpc<C: Command, CE: CommandExecutor<C>, RC: RoleChange> {
    /// The inner server is wrapped in an Arc so that its state can be shared while cloning the rpc wrapper
    inner: Arc<CurpNode<C, CE, RC>>,
}

impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> Clone for Rpc<C, CE, RC> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[tonic::async_trait]
impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> crate::rpc::Protocol for Rpc<C, CE, RC> {
    type ProposeStreamStream = RecvStream<'static, Result<OpResponse, Status>>;

    #[instrument(skip_all, name = "propose_stream")]
    async fn propose_stream(
        &self,
        request: tonic::Request<ProposeRequest>,
    ) -> Result<tonic::Response<Self::ProposeStreamStream>, Status> {
        let bypassed = request.metadata().is_bypassed();
        let (tx, rx) = flume::bounded(2);
        let resp_tx = Arc::new(ResponseSender::new(tx));
        self.inner
            .propose_stream(&request.into_inner(), resp_tx, bypassed)
            .await?;

        Ok(tonic::Response::new(rx.into_stream()))
    }

    #[instrument(skip_all, name = "record")]
    async fn record(
        &self,
        request: tonic::Request<RecordRequest>,
    ) -> Result<tonic::Response<RecordResponse>, Status> {
        Ok(tonic::Response::new(
            self.inner.record(&request.into_inner())?,
        ))
    }

    #[instrument(skip_all, name = "read_index")]
    async fn read_index(
        &self,
        _request: tonic::Request<ReadIndexRequest>,
    ) -> Result<tonic::Response<ReadIndexResponse>, Status> {
        Ok(tonic::Response::new(self.inner.read_index()?))
    }

    #[instrument(skip_all, name = "curp_shutdown")]
    async fn shutdown(
        &self,
        request: tonic::Request<ShutdownRequest>,
    ) -> Result<tonic::Response<ShutdownResponse>, Status> {
        let bypassed = request.metadata().is_bypassed();
        request.metadata().extract_span();
        Ok(tonic::Response::new(
            self.inner.shutdown(request.into_inner(), bypassed).await?,
        ))
    }

    #[instrument(skip_all, name = "curp_propose_conf_change")]
    async fn propose_conf_change(
        &self,
        request: tonic::Request<ProposeConfChangeRequest>,
    ) -> Result<tonic::Response<ProposeConfChangeResponse>, Status> {
        let bypassed = request.metadata().is_bypassed();
        request.metadata().extract_span();
        Ok(tonic::Response::new(
            self.inner
                .propose_conf_change(request.into_inner(), bypassed)
                .await?,
        ))
    }

    #[instrument(skip_all, name = "curp_publish")]
    async fn publish(
        &self,
        request: tonic::Request<PublishRequest>,
    ) -> Result<tonic::Response<PublishResponse>, Status> {
        let bypassed = request.metadata().is_bypassed();
        request.metadata().extract_span();
        Ok(tonic::Response::new(
            self.inner.publish(request.into_inner(), bypassed)?,
        ))
    }

    #[instrument(skip_all, name = "curp_fetch_cluster")]
    async fn fetch_cluster(
        &self,
        request: tonic::Request<FetchClusterRequest>,
    ) -> Result<tonic::Response<FetchClusterResponse>, Status> {
        Ok(tonic::Response::new(
            self.inner.fetch_cluster(request.into_inner())?,
        ))
    }

    #[instrument(skip_all, name = "curp_fetch_read_state")]
    async fn fetch_read_state(
        &self,
        request: tonic::Request<FetchReadStateRequest>,
    ) -> Result<tonic::Response<FetchReadStateResponse>, Status> {
        Ok(tonic::Response::new(
            self.inner.fetch_read_state(request.into_inner())?,
        ))
    }

    #[instrument(skip_all, name = "curp_move_leader")]
    async fn move_leader(
        &self,
        request: tonic::Request<MoveLeaderRequest>,
    ) -> Result<tonic::Response<MoveLeaderResponse>, Status> {
        Ok(tonic::Response::new(
            self.inner.move_leader(request.into_inner()).await?,
        ))
    }

    #[instrument(skip_all, name = "lease_keep_alive")]
    async fn lease_keep_alive(
        &self,
        request: tonic::Request<tonic::Streaming<LeaseKeepAliveMsg>>,
    ) -> Result<tonic::Response<LeaseKeepAliveMsg>, Status> {
        let req_stream = request.into_inner();
        Ok(tonic::Response::new(
            self.inner.lease_keep_alive(req_stream).await?,
        ))
    }
}

#[tonic::async_trait]
impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> crate::rpc::InnerProtocol
    for Rpc<C, CE, RC>
{
    #[instrument(skip_all, name = "curp_append_entries")]
    async fn append_entries(
        &self,
        request: tonic::Request<AppendEntriesRequest>,
    ) -> Result<tonic::Response<AppendEntriesResponse>, Status> {
        Ok(tonic::Response::new(
            self.inner.append_entries(request.get_ref())?,
        ))
    }

    #[instrument(skip_all, name = "curp_vote")]
    async fn vote(
        &self,
        request: tonic::Request<VoteRequest>,
    ) -> Result<tonic::Response<VoteResponse>, Status> {
        Ok(tonic::Response::new(
            self.inner.vote(&request.into_inner())?,
        ))
    }

    #[instrument(skip_all, name = "curp_trigger_shutdown")]
    async fn trigger_shutdown(
        &self,
        request: tonic::Request<TriggerShutdownRequest>,
    ) -> Result<tonic::Response<TriggerShutdownResponse>, Status> {
        Ok(tonic::Response::new(
            self.inner.trigger_shutdown(*request.get_ref()),
        ))
    }

    #[instrument(skip_all, name = "curp_install_snapshot")]
    async fn install_snapshot(
        &self,
        request: tonic::Request<tonic::Streaming<InstallSnapshotRequest>>,
    ) -> Result<tonic::Response<InstallSnapshotResponse>, Status> {
        let req_stream = request.into_inner();
        Ok(tonic::Response::new(
            self.inner.install_snapshot(req_stream).await?,
        ))
    }

    #[instrument(skip_all, name = "curp_try_become_leader_now")]
    async fn try_become_leader_now(
        &self,
        request: tonic::Request<TryBecomeLeaderNowRequest>,
    ) -> Result<tonic::Response<TryBecomeLeaderNowResponse>, Status> {
        Ok(tonic::Response::new(
            self.inner.try_become_leader_now(request.get_ref()).await?,
        ))
    }
}

impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> Rpc<C, CE, RC> {
    /// New `Rpc`
    ///
    /// # Panics
    ///
    /// Panic if storage creation failed
    #[inline]
    #[allow(clippy::too_many_arguments)] // TODO: refactor this use builder pattern
    pub fn new(
        cluster_info: Arc<ClusterInfo>,
        is_leader: bool,
        executor: Arc<CE>,
        snapshot_allocator: Box<dyn SnapshotAllocator>,
        role_change: RC,
        curp_cfg: Arc<CurpConfig>,
        storage: Arc<DB<C>>,
        task_manager: Arc<TaskManager>,
        client_tls_config: Option<ClientTlsConfig>,
        sps: Vec<SpObject<C>>,
        ucps: Vec<UcpObject<C>>,
    ) -> Self {
        Self::new_inner(
            cluster_info,
            is_leader,
            executor,
            snapshot_allocator,
            role_change,
            curp_cfg,
            storage,
            task_manager,
            client_tls_config,
            sps,
            ucps,
            crate::rpc::TransportConfig::default(),
        )
    }

    /// Create a new `Rpc` with QUIC transport
    ///
    /// This only creates the `Rpc` instance. To start the QUIC server,
    /// call `QuicGrpcServer::new(rpc).serve(listeners)` separately.
    #[cfg(feature = "quic")]
    #[allow(dead_code)] // Will be used in integration tests
    pub fn new_with_quic(
        cluster_info: Arc<ClusterInfo>,
        is_leader: bool,
        executor: Arc<CE>,
        snapshot_allocator: Box<dyn SnapshotAllocator>,
        role_change: RC,
        curp_cfg: Arc<CurpConfig>,
        storage: Arc<DB<C>>,
        task_manager: Arc<TaskManager>,
        client_tls_config: Option<ClientTlsConfig>,
        sps: Vec<SpObject<C>>,
        ucps: Vec<UcpObject<C>>,
        quic_client: Arc<gm_quic::prelude::QuicClient>,
    ) -> Self {
        Self::new_inner(
            cluster_info,
            is_leader,
            executor,
            snapshot_allocator,
            role_change,
            curp_cfg,
            storage,
            task_manager,
            client_tls_config,
            sps,
            ucps,
            crate::rpc::TransportConfig::Quic(
                quic_client,
                crate::rpc::quic_transport::channel::DnsFallback::Disabled,
            ),
        )
    }

    /// Create a new `Rpc` with QUIC transport and localhost DNS fallback (test only)
    ///
    /// Same as `new_with_quic` but enables localhost fallback for fake hostnames.
    #[cfg(all(feature = "quic", any(test, feature = "quic-test")))]
    #[allow(dead_code)]
    pub fn new_with_quic_for_test(
        cluster_info: Arc<ClusterInfo>,
        is_leader: bool,
        executor: Arc<CE>,
        snapshot_allocator: Box<dyn SnapshotAllocator>,
        role_change: RC,
        curp_cfg: Arc<CurpConfig>,
        storage: Arc<DB<C>>,
        task_manager: Arc<TaskManager>,
        client_tls_config: Option<ClientTlsConfig>,
        sps: Vec<SpObject<C>>,
        ucps: Vec<UcpObject<C>>,
        quic_client: Arc<gm_quic::prelude::QuicClient>,
    ) -> Self {
        Self::new_inner(
            cluster_info,
            is_leader,
            executor,
            snapshot_allocator,
            role_change,
            curp_cfg,
            storage,
            task_manager,
            client_tls_config,
            sps,
            ucps,
            crate::rpc::TransportConfig::Quic(
                quic_client,
                crate::rpc::quic_transport::channel::DnsFallback::LocalhostForTest,
            ),
        )
    }

    /// Internal constructor with explicit transport configuration
    fn new_inner(
        cluster_info: Arc<ClusterInfo>,
        is_leader: bool,
        executor: Arc<CE>,
        snapshot_allocator: Box<dyn SnapshotAllocator>,
        role_change: RC,
        curp_cfg: Arc<CurpConfig>,
        storage: Arc<DB<C>>,
        task_manager: Arc<TaskManager>,
        client_tls_config: Option<ClientTlsConfig>,
        sps: Vec<SpObject<C>>,
        ucps: Vec<UcpObject<C>>,
        transport: crate::rpc::TransportConfig,
    ) -> Self {
        #[allow(clippy::panic)]
        let curp_node = match CurpNode::new_with_transport(
            cluster_info,
            is_leader,
            executor,
            snapshot_allocator,
            role_change,
            curp_cfg,
            storage,
            task_manager,
            client_tls_config,
            sps,
            ucps,
            transport,
        ) {
            Ok(n) => n,
            Err(err) => {
                panic!("failed to create curp service, {err:?}");
            }
        };

        Self {
            inner: Arc::new(curp_node),
        }
    }

    /// Get a subscriber for leader changes
    #[inline]
    #[must_use]
    pub fn leader_rx(&self) -> broadcast::Receiver<Option<ServerId>> {
        self.inner.leader_rx()
    }

    /// Get raw curp
    #[inline]
    #[must_use]
    pub fn raw_curp(&self) -> Arc<RawCurp<C, RC>> {
        self.inner.raw_curp()
    }
}

// QUIC transport service trait implementations
#[cfg(feature = "quic")]
mod quic_service_impl {
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures::{Stream, StreamExt};

    use crate::{
        cmd::{Command, CommandExecutor},
        response::ResponseSender,
        role_change::RoleChange,
        rpc::{
            AppendEntriesRequest, AppendEntriesResponse, CurpError, FetchClusterRequest,
            FetchClusterResponse, FetchReadStateRequest, FetchReadStateResponse,
            InstallSnapshotRequest, InstallSnapshotResponse, LeaseKeepAliveMsg, Metadata,
            MoveLeaderRequest, MoveLeaderResponse, OpResponse, ProposeConfChangeRequest,
            ProposeConfChangeResponse, ProposeRequest, PublishRequest, PublishResponse,
            ReadIndexResponse, RecordRequest, RecordResponse, ShutdownRequest, ShutdownResponse,
            VoteRequest, VoteResponse,
        },
    };

    use super::Rpc;

    #[async_trait]
    impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> crate::rpc::CurpService
        for Rpc<C, CE, RC>
    {
        async fn propose_stream(
            &self,
            req: ProposeRequest,
            meta: Metadata,
        ) -> Result<Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send + Unpin>, CurpError>
        {
            let bypassed = meta.is_bypassed();
            let (tx, rx) = flume::bounded(2);
            let resp_tx = Arc::new(ResponseSender::new(tx));
            self.inner.propose_stream(&req, resp_tx, bypassed).await?;

            // Convert flume stream to boxed stream with CurpError
            let stream = rx.into_stream().map(|r| r.map_err(CurpError::from));
            Ok(Box::new(stream))
        }

        fn record(&self, req: RecordRequest, _meta: Metadata) -> Result<RecordResponse, CurpError> {
            self.inner.record(&req).map_err(CurpError::from)
        }

        fn read_index(&self, _meta: Metadata) -> Result<ReadIndexResponse, CurpError> {
            self.inner.read_index().map_err(CurpError::from)
        }

        async fn shutdown(
            &self,
            req: ShutdownRequest,
            meta: Metadata,
        ) -> Result<ShutdownResponse, CurpError> {
            let bypassed = meta.is_bypassed();
            self.inner
                .shutdown(req, bypassed)
                .await
                .map_err(CurpError::from)
        }

        async fn propose_conf_change(
            &self,
            req: ProposeConfChangeRequest,
            meta: Metadata,
        ) -> Result<ProposeConfChangeResponse, CurpError> {
            let bypassed = meta.is_bypassed();
            self.inner
                .propose_conf_change(req, bypassed)
                .await
                .map_err(CurpError::from)
        }

        fn publish(
            &self,
            req: PublishRequest,
            meta: Metadata,
        ) -> Result<PublishResponse, CurpError> {
            let bypassed = meta.is_bypassed();
            self.inner.publish(req, bypassed).map_err(CurpError::from)
        }

        fn fetch_cluster(
            &self,
            req: FetchClusterRequest,
        ) -> Result<FetchClusterResponse, CurpError> {
            self.inner.fetch_cluster(req).map_err(CurpError::from)
        }

        fn fetch_read_state(
            &self,
            req: FetchReadStateRequest,
        ) -> Result<FetchReadStateResponse, CurpError> {
            self.inner.fetch_read_state(req).map_err(CurpError::from)
        }

        async fn move_leader(
            &self,
            req: MoveLeaderRequest,
        ) -> Result<MoveLeaderResponse, CurpError> {
            self.inner.move_leader(req).await.map_err(CurpError::from)
        }

        async fn lease_keep_alive(
            &self,
            stream: Box<dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin>,
        ) -> Result<LeaseKeepAliveMsg, CurpError> {
            // Convert CurpError stream to tonic::Status stream for inner implementation
            let tonic_stream = stream.map(|r| r.map_err(|e| tonic::Status::from(e)));
            self.inner
                .lease_keep_alive(tonic_stream)
                .await
                .map_err(CurpError::from)
        }
    }

    #[async_trait]
    impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> crate::rpc::InnerCurpService
        for Rpc<C, CE, RC>
    {
        fn append_entries(
            &self,
            req: AppendEntriesRequest,
        ) -> Result<AppendEntriesResponse, CurpError> {
            self.inner.append_entries(&req).map_err(CurpError::from)
        }

        fn vote(&self, req: VoteRequest) -> Result<VoteResponse, CurpError> {
            self.inner.vote(&req).map_err(CurpError::from)
        }

        async fn install_snapshot(
            &self,
            stream: Box<
                dyn Stream<Item = Result<InstallSnapshotRequest, CurpError>> + Send + Unpin,
            >,
        ) -> Result<InstallSnapshotResponse, CurpError> {
            // Convert CurpError stream to tonic::Status stream for inner implementation
            let tonic_stream = stream.map(|r| r.map_err(|e| tonic::Status::from(e)));
            self.inner
                .install_snapshot(tonic_stream)
                .await
                .map_err(CurpError::from)
        }

        fn trigger_shutdown(&self) -> Result<(), CurpError> {
            use crate::rpc::TriggerShutdownRequest;
            let _resp = self.inner.trigger_shutdown(TriggerShutdownRequest {});
            Ok(())
        }

        async fn try_become_leader_now(&self) -> Result<(), CurpError> {
            use crate::rpc::TryBecomeLeaderNowRequest;
            let _ = self
                .inner
                .try_become_leader_now(&TryBecomeLeaderNowRequest {})
                .await
                .map_err(CurpError::from)?;
            Ok(())
        }
    }
}
