use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;

use crate::rpc::{
    AppendEntriesRequest, AppendEntriesResponse, CurpError, FetchClusterRequest,
    FetchClusterResponse, FetchReadStateRequest, FetchReadStateResponse, InstallSnapshotRequest,
    InstallSnapshotResponse, LeaseKeepAliveMsg, MoveLeaderRequest, MoveLeaderResponse, OpResponse,
    ProposeConfChangeRequest, ProposeConfChangeResponse, ProposeRequest, PublishRequest,
    PublishResponse, ReadIndexResponse, RecordRequest, RecordResponse, ShutdownRequest,
    ShutdownResponse, VoteRequest, VoteResponse,
};

/// Curp Server core abstraction
#[async_trait]
pub trait CurpServer: Send + Sync + 'static {
    /// Handle propose stream request
    async fn handle_propose_stream(
        &self,
        req: ProposeRequest,
        bypassed: bool,
    ) -> Result<Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send + Unpin>, CurpError>;
    
    /// Handle record request
    fn handle_record(&self, req: RecordRequest) -> Result<RecordResponse, CurpError>;
    
    /// Handle read index request
    fn handle_read_index(&self) -> Result<ReadIndexResponse, CurpError>;
    
    /// Handle shutdown request
    async fn handle_shutdown(
        &self,
        req: ShutdownRequest,
        bypassed: bool,
    ) -> Result<ShutdownResponse, CurpError>;
    
    /// Handle configuration change request
    async fn handle_propose_conf_change(
        &self,
        req: ProposeConfChangeRequest,
        bypassed: bool,
    ) -> Result<ProposeConfChangeResponse, CurpError>;
    
    /// Handle publish request
    fn handle_publish(
        &self,
        req: PublishRequest,
        bypassed: bool,
    ) -> Result<PublishResponse, CurpError>;
    
    /// Handle fetch cluster request
    fn handle_fetch_cluster(&self, req: FetchClusterRequest) -> Result<FetchClusterResponse, CurpError>;
    
    /// Handle fetch read state request
    fn handle_fetch_read_state(
        &self,
        req: FetchReadStateRequest,
    ) -> Result<FetchReadStateResponse, CurpError>;
    
    /// Handle move leader request
    async fn handle_move_leader(
        &self,
        req: MoveLeaderRequest,
    ) -> Result<MoveLeaderResponse, CurpError>;
    
    /// Handle lease keep alive stream
    async fn handle_lease_keep_alive(
        &self,
        stream: Box<dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin>,
    ) -> Result<LeaseKeepAliveMsg, CurpError>;
    
    /// Handle append entries request
    fn handle_append_entries(
        &self,
        req: AppendEntriesRequest,
    ) -> Result<AppendEntriesResponse, CurpError>;
    
    /// Handle vote request
    fn handle_vote(&self, req: VoteRequest) -> Result<VoteResponse, CurpError>;
    
    /// Handle install snapshot stream
    async fn handle_install_snapshot(
        &self,
        stream: Box<dyn Stream<Item = Result<InstallSnapshotRequest, CurpError>> + Send + Unpin>,
    ) -> Result<InstallSnapshotResponse, CurpError>;
    
    /// Handle trigger shutdown request
    fn handle_trigger_shutdown(&self) -> Result<(), CurpError>;
    
    /// Handle try become leader now request
    async fn handle_try_become_leader_now(&self) -> Result<(), CurpError>;
}