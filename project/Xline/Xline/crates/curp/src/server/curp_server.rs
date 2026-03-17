use std::sync::Arc;

use async_trait::async_trait;
use futures::{Stream, StreamExt};

use crate::{cmd::{Command, CommandExecutor}, role_change::RoleChange};
use crate::rpc::{
    AppendEntriesRequest, AppendEntriesResponse, CurpError, FetchClusterRequest,
    FetchClusterResponse, FetchReadStateRequest, FetchReadStateResponse, InstallSnapshotRequest,
    InstallSnapshotResponse, LeaseKeepAliveMsg, MoveLeaderRequest, MoveLeaderResponse, OpResponse,
    ProposeConfChangeRequest, ProposeConfChangeResponse, ProposeRequest, PublishRequest,
    PublishResponse, ReadIndexResponse, RecordRequest, RecordResponse, ShutdownRequest,
    ShutdownResponse, VoteRequest, VoteResponse,
};
use xlinerpc::status::Status;

use super::{CurpServer, curp_node::CurpNode};

/// CurpServer concrete implementation
pub struct CurpServerImpl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> {
    /// Inner CurpNode instance
    inner: Arc<CurpNode<C, CE, RC>>,
}

impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> CurpServerImpl<C, CE, RC> {
    /// Create a new CurpServerImpl
    pub fn new(inner: Arc<CurpNode<C, CE, RC>>) -> Self {
        Self {
            inner,
        }
    }
    
    /// Get inner CurpNode
    pub fn inner(&self) -> &Arc<CurpNode<C, CE, RC>> {
        &self.inner
    }
}

#[async_trait]
impl<C: Command, CE: CommandExecutor<C>, RC: RoleChange> CurpServer for CurpServerImpl<C, CE, RC> {
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