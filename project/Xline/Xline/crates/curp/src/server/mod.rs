use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use futures::{Stream, StreamExt};

use self::curp_node::CurpNode;
use crate::rpc::{
    AppendEntriesRequest, AppendEntriesResponse, CurpError, FetchClusterRequest,
    FetchClusterResponse, FetchReadStateRequest, FetchReadStateResponse, InstallSnapshotRequest,
    InstallSnapshotResponse, LeaseKeepAliveMsg, MoveLeaderRequest, MoveLeaderResponse, OpResponse,
    ProposeConfChangeRequest, ProposeConfChangeResponse, ProposeRequest, PublishRequest,
    PublishResponse, ReadIndexResponse, RecordRequest, RecordResponse, ShutdownRequest,
    ShutdownResponse, VoteRequest, VoteResponse,
};
pub use self::{
    conflict::{spec_pool_new::SpObject, uncommitted_pool::UcpObject},
    raw_curp::RawCurp,
};
use crate::{
    cmd::{Command, CommandExecutor},
    members::{ClusterInfo, ServerId},
    role_change::RoleChange,
};
use engine::SnapshotAllocator;
use tokio::sync::broadcast;
use utils::{config::CurpConfig, task_manager::TaskManager};

// Import new Server abstraction
mod server;
mod curp_server;
mod registry;
mod router;
mod adapter;

// Export core components
pub use self::server::CurpServer;
pub use self::curp_server::CurpServerImpl;
pub use self::registry::CurpServiceRegistry;
pub use self::router::CurpRouter;
pub use self::adapter::CurpProtocolAdapter;

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

// ============================================================================
// Rpc constructors and methods
// ============================================================================

impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> Rpc<C, CE, RC> {
    /// Create a new `Rpc` with QUIC transport
    ///
    /// This only creates the `Rpc` instance. To start the QUIC server,
    /// call `QuicGrpcServer::new(rpc).serve(listeners)` separately.
    pub fn new(
        cluster_info: Arc<ClusterInfo>,
        is_leader: bool,
        executor: Arc<CE>,
        snapshot_allocator: Box<dyn SnapshotAllocator>,
        role_change: RC,
        curp_cfg: Arc<CurpConfig>,
        storage: Arc<DB<C>>,
        task_manager: Arc<TaskManager>,
        sps: Vec<SpObject<C>>,
        ucps: Vec<UcpObject<C>>,
        quic_client: Arc<gm_quic::prelude::QuicClient>,
    ) -> Result<Self, crate::rpc::CurpError> {
        Self::new_inner(
            cluster_info,
            is_leader,
            executor,
            snapshot_allocator,
            role_change,
            curp_cfg,
            storage,
            task_manager,
            sps,
            ucps,
            crate::rpc::TransportConfig {
                client: quic_client,
                dns_fallback: crate::rpc::quic_transport::channel::DnsFallback::Disabled,
            },
        )
    }

    /// Create a new `Rpc` with QUIC transport and localhost DNS fallback (test only)
    ///
    /// Same as `new` but enables localhost fallback for fake hostnames.
    #[doc(hidden)]
    pub fn new_for_test(
        cluster_info: Arc<ClusterInfo>,
        is_leader: bool,
        executor: Arc<CE>,
        snapshot_allocator: Box<dyn SnapshotAllocator>,
        role_change: RC,
        curp_cfg: Arc<CurpConfig>,
        storage: Arc<DB<C>>,
        task_manager: Arc<TaskManager>,
        sps: Vec<SpObject<C>>,
        ucps: Vec<UcpObject<C>>,
        quic_client: Arc<gm_quic::prelude::QuicClient>,
    ) -> Result<Self, crate::rpc::CurpError> {
        Self::new_inner(
            cluster_info,
            is_leader,
            executor,
            snapshot_allocator,
            role_change,
            curp_cfg,
            storage,
            task_manager,
            sps,
            ucps,
            crate::rpc::TransportConfig {
                client: quic_client,
                dns_fallback: crate::rpc::quic_transport::channel::DnsFallback::LocalhostForTest,
            },
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
        sps: Vec<SpObject<C>>,
        ucps: Vec<UcpObject<C>>,
        transport: crate::rpc::TransportConfig,
    ) -> Result<Self, crate::rpc::CurpError> {
        let curp_node = CurpNode::new_with_transport(
            cluster_info,
            is_leader,
            executor,
            snapshot_allocator,
            role_change,
            curp_cfg,
            storage,
            task_manager,
            sps,
            ucps,
            transport,
        )?;

        Ok(Self {
            inner: Arc::new(curp_node),
        })
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

// ============================================================================
// CurpServer trait implementation for Rpc
// ============================================================================

#[async_trait]
impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> CurpServer for Rpc<C, CE, RC> {
    async fn handle_propose_stream(
        &self,
        req: ProposeRequest,
        bypassed: bool,
    ) -> Result<Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send + Unpin>, CurpError> {
        let (tx, rx) = flume::bounded(2);
        let resp_tx = Arc::new(crate::response::ResponseSender::new(tx));
        self.inner.propose_stream(&req, resp_tx, bypassed).await?;

        let stream = rx.into_stream().map(|r| r.map_err(CurpError::from));
        Ok(Box::new(stream))
    }
    
    fn handle_record(&self, req: RecordRequest) -> Result<RecordResponse, CurpError> {
        self.inner.record(&req).map_err(CurpError::from)
    }
    
    fn handle_read_index(&self) -> Result<ReadIndexResponse, CurpError> {
        self.inner.read_index().map_err(CurpError::from)
    }
    
    async fn handle_shutdown(
        &self,
        req: ShutdownRequest,
        bypassed: bool,
    ) -> Result<ShutdownResponse, CurpError> {
        self.inner.shutdown(req, bypassed).await.map_err(CurpError::from)
    }
    
    async fn handle_propose_conf_change(
        &self,
        req: ProposeConfChangeRequest,
        bypassed: bool,
    ) -> Result<ProposeConfChangeResponse, CurpError> {
        self.inner.propose_conf_change(req, bypassed).await.map_err(CurpError::from)
    }
    
    fn handle_publish(
        &self,
        req: PublishRequest,
        bypassed: bool,
    ) -> Result<PublishResponse, CurpError> {
        self.inner.publish(req, bypassed).map_err(CurpError::from)
    }
    
    fn handle_fetch_cluster(&self, req: FetchClusterRequest) -> Result<FetchClusterResponse, CurpError> {
        self.inner.fetch_cluster(req).map_err(CurpError::from)
    }
    
    fn handle_fetch_read_state(
        &self,
        req: FetchReadStateRequest,
    ) -> Result<FetchReadStateResponse, CurpError> {
        self.inner.fetch_read_state(req).map_err(CurpError::from)
    }
    
    async fn handle_move_leader(
        &self,
        req: MoveLeaderRequest,
    ) -> Result<MoveLeaderResponse, CurpError> {
        self.inner.move_leader(req).await.map_err(CurpError::from)
    }
    
    async fn handle_lease_keep_alive(
        &self,
        stream: Box<dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin>,
    ) -> Result<LeaseKeepAliveMsg, CurpError> {
        // CurpNode::lease_keep_alive is generic over E: Error + 'static.
        // xlinerpc::Status implements Error, so convert CurpError → xlinerpc::Status.
        let status_stream = stream.map(|r| r.map_err(xlinerpc::status::Status::from));
        self.inner.lease_keep_alive(status_stream).await.map_err(CurpError::from)
    }
    
    fn handle_append_entries(
        &self,
        req: AppendEntriesRequest,
    ) -> Result<AppendEntriesResponse, CurpError> {
        self.inner.append_entries(&req).map_err(CurpError::from)
    }
    
    fn handle_vote(&self, req: VoteRequest) -> Result<VoteResponse, CurpError> {
        self.inner.vote(&req).map_err(CurpError::from)
    }
    
    async fn handle_install_snapshot(
        &self,
        stream: Box<dyn Stream<Item = Result<InstallSnapshotRequest, CurpError>> + Send + Unpin>,
    ) -> Result<InstallSnapshotResponse, CurpError> {
        // CurpNode::install_snapshot is generic over E: Error + 'static.
        // xlinerpc::Status implements Error, so convert CurpError → xlinerpc::Status.
        let status_stream = stream.map(|r| r.map_err(xlinerpc::status::Status::from));
        self.inner.install_snapshot(status_stream).await.map_err(CurpError::from)
    }
    
    fn handle_trigger_shutdown(&self) -> Result<(), CurpError> {
        use crate::rpc::TriggerShutdownRequest;
        let _resp = self.inner.trigger_shutdown(TriggerShutdownRequest {});
        Ok(())
    }
    
    async fn handle_try_become_leader_now(&self) -> Result<(), CurpError> {
        use crate::rpc::TryBecomeLeaderNowRequest;
        self.inner.try_become_leader_now(TryBecomeLeaderNowRequest {}).await.map_err(CurpError::from)
    }
}

// ============================================================================
// QUIC transport service trait implementations
// ============================================================================

mod quic_service_impl {
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures::{Stream, StreamExt};
    use utils::tracing::Extract;

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

    use super::{Rpc, CurpServer, CurpProtocolAdapter, CurpRouter, CurpServiceRegistry};

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
            self.handle_propose_stream(req, bypassed).await
        }

        fn record(&self, req: RecordRequest, _meta: Metadata) -> Result<RecordResponse, CurpError> {
            self.handle_record(req)
        }

        fn read_index(&self, _meta: Metadata) -> Result<ReadIndexResponse, CurpError> {
            self.handle_read_index()
        }

        async fn shutdown(
            &self,
            req: ShutdownRequest,
            meta: Metadata,
        ) -> Result<ShutdownResponse, CurpError> {
            let bypassed = meta.is_bypassed();
            meta.extract_span();
            self.handle_shutdown(req, bypassed).await
        }

        async fn propose_conf_change(
            &self,
            req: ProposeConfChangeRequest,
            meta: Metadata,
        ) -> Result<ProposeConfChangeResponse, CurpError> {
            let bypassed = meta.is_bypassed();
            meta.extract_span();
            self.handle_propose_conf_change(req, bypassed).await
        }

        fn publish(
            &self,
            req: PublishRequest,
            meta: Metadata,
        ) -> Result<PublishResponse, CurpError> {
            let bypassed = meta.is_bypassed();
            meta.extract_span();
            self.handle_publish(req, bypassed)
        }

        fn fetch_cluster(
            &self,
            req: FetchClusterRequest,
        ) -> Result<FetchClusterResponse, CurpError> {
            self.handle_fetch_cluster(req)
        }

        fn fetch_read_state(
            &self,
            req: FetchReadStateRequest,
        ) -> Result<FetchReadStateResponse, CurpError> {
            self.handle_fetch_read_state(req)
        }

        async fn move_leader(
            &self,
            req: MoveLeaderRequest,
        ) -> Result<MoveLeaderResponse, CurpError> {
            self.handle_move_leader(req).await
        }

        async fn lease_keep_alive(
            &self,
            stream: Box<dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin>,
        ) -> Result<LeaseKeepAliveMsg, CurpError> {
            self.handle_lease_keep_alive(stream).await
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
            self.handle_append_entries(req)
        }

        fn vote(&self, req: VoteRequest) -> Result<VoteResponse, CurpError> {
            self.handle_vote(req)
        }

        async fn install_snapshot(
            &self,
            stream: Box<
                dyn Stream<Item = Result<InstallSnapshotRequest, CurpError>> + Send + Unpin,
            >,
        ) -> Result<InstallSnapshotResponse, CurpError> {
            self.handle_install_snapshot(stream).await
        }

        fn trigger_shutdown(&self) -> Result<(), CurpError> {
            self.handle_trigger_shutdown()
        }

        async fn try_become_leader_now(&self) -> Result<(), CurpError> {
            self.handle_try_become_leader_now().await
        }
    }
}

// ============================================================================// Server 抽象适配实现// ============================================================================

#[async_trait]
impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> CurpServer for Rpc<C, CE, RC> {
    async fn handle_propose_stream(
        &self,
        req: ProposeRequest,
        bypassed: bool,
    ) -> Result<Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send + Unpin>, CurpError> {
        let (tx, rx) = flume::bounded(2);
        let resp_tx = Arc::new(crate::response::ResponseSender::new(tx));
        self.inner.propose_stream(&req, resp_tx, bypassed).await?;

        let stream = rx.into_stream().map(|r| r.map_err(CurpError::from));
        Ok(Box::new(stream))
    }
    
    fn handle_record(&self, req: RecordRequest) -> Result<RecordResponse, CurpError> {
        self.inner.record(&req).map_err(CurpError::from)
    }
    
    fn handle_read_index(&self) -> Result<ReadIndexResponse, CurpError> {
        self.inner.read_index().map_err(CurpError::from)
    }
    
    async fn handle_shutdown(
        &self,
        req: ShutdownRequest,
        bypassed: bool,
    ) -> Result<ShutdownResponse, CurpError> {
        self.inner.shutdown(req, bypassed).await.map_err(CurpError::from)
    }
    
    async fn handle_propose_conf_change(
        &self,
        req: ProposeConfChangeRequest,
        bypassed: bool,
    ) -> Result<ProposeConfChangeResponse, CurpError> {
        self.inner.propose_conf_change(req, bypassed).await.map_err(CurpError::from)
    }
    
    fn handle_publish(
        &self,
        req: PublishRequest,
        bypassed: bool,
    ) -> Result<PublishResponse, CurpError> {
        self.inner.publish(req, bypassed).map_err(CurpError::from)
    }
    
    fn handle_fetch_cluster(&self, req: FetchClusterRequest) -> Result<FetchClusterResponse, CurpError> {
        self.inner.fetch_cluster(req).map_err(CurpError::from)
    }
    
    fn handle_fetch_read_state(
        &self,
        req: FetchReadStateRequest,
    ) -> Result<FetchReadStateResponse, CurpError> {
        self.inner.fetch_read_state(req).map_err(CurpError::from)
    }
    
    async fn handle_move_leader(
        &self,
        req: MoveLeaderRequest,
    ) -> Result<MoveLeaderResponse, CurpError> {
        self.inner.move_leader(req).await.map_err(CurpError::from)
    }
    
    async fn handle_lease_keep_alive(
        &self,
        stream: Box<dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin>,
    ) -> Result<LeaseKeepAliveMsg, CurpError> {
        // CurpNode::lease_keep_alive is generic over E: Error + 'static.
        // xlinerpc::Status implements Error, so convert CurpError → xlinerpc::Status.
        let status_stream = stream.map(|r| r.map_err(xlinerpc::status::Status::from));
        self.inner.lease_keep_alive(status_stream).await.map_err(CurpError::from)
    }
    
    fn handle_append_entries(
        &self,
        req: AppendEntriesRequest,
    ) -> Result<AppendEntriesResponse, CurpError> {
        self.inner.append_entries(&req).map_err(CurpError::from)
    }
    
    fn handle_vote(&self, req: VoteRequest) -> Result<VoteResponse, CurpError> {
        self.inner.vote(&req).map_err(CurpError::from)
    }
    
    async fn handle_install_snapshot(
        &self,
        stream: Box<dyn Stream<Item = Result<InstallSnapshotRequest, CurpError>> + Send + Unpin>,
    ) -> Result<InstallSnapshotResponse, CurpError> {
        // CurpNode::install_snapshot is generic over E: Error + 'static.
        // xlinerpc::Status implements Error, so convert CurpError → xlinerpc::Status.
        let status_stream = stream.map(|r| r.map_err(xlinerpc::status::Status::from));
        self.inner.install_snapshot(status_stream).await.map_err(CurpError::from)
    }
    
    fn handle_trigger_shutdown(&self) -> Result<(), CurpError> {
        use crate::rpc::TriggerShutdownRequest;
        let _resp = self.inner.trigger_shutdown(TriggerShutdownRequest {});
        Ok(())
    }
    
    async fn handle_try_become_leader_now(&self) -> Result<(), CurpError> {
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