use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;

use crate::rpc::{
    AppendEntriesRequest, AppendEntriesResponse, CurpError, FetchClusterRequest,
    FetchClusterResponse, FetchReadStateRequest, FetchReadStateResponse, InstallSnapshotRequest,
    InstallSnapshotResponse, LeaseKeepAliveMsg, Metadata, MoveLeaderRequest, MoveLeaderResponse, OpResponse,
    ProposeConfChangeRequest, ProposeConfChangeResponse, ProposeRequest, PublishRequest,
    PublishResponse, ReadIndexResponse, RecordRequest, RecordResponse, ShutdownRequest,
    ShutdownResponse, VoteRequest, VoteResponse, CurpService, InnerCurpService,
};

use super::{CurpRouter, CurpServer};

/// Curp protocol adapter
/// Adapts CurpService trait calls to CurpServer trait
pub struct CurpProtocolAdapter {
    /// 路由器
    router: Arc<CurpRouter>,
    /// 服务名称
    service_name: String,
}

impl CurpProtocolAdapter {
    /// Create a new protocol adapter
    pub fn new(router: Arc<CurpRouter>, service_name: String) -> Self {
        Self {
            router,
            service_name,
        }
    }
    
    /// Get service
    fn get_service(&self) -> Result<Arc<dyn CurpServer>, CurpError> {
        self.router.registry().get_service(&self.service_name)
            .ok_or_else(|| CurpError::Internal(format!("Service '{}' not found", self.service_name)))
    }
}

#[async_trait]
impl CurpService for CurpProtocolAdapter {
    async fn propose_stream(
        &self,
        req: ProposeRequest,
        meta: Metadata,
    ) -> Result<Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send + Unpin>, CurpError> {
        let bypassed = meta.is_bypassed();
        let service = self.get_service()?;
        service.handle_propose_stream(req, bypassed).await
    }
    
    fn record(&self, req: RecordRequest, _meta: Metadata) -> Result<RecordResponse, CurpError> {
        let service = self.get_service()?;
        service.handle_record(req)
    }
    
    fn read_index(&self, _meta: Metadata) -> Result<ReadIndexResponse, CurpError> {
        let service = self.get_service()?;
        service.handle_read_index()
    }
    
    async fn shutdown(
        &self,
        req: ShutdownRequest,
        meta: Metadata,
    ) -> Result<ShutdownResponse, CurpError> {
        let bypassed = meta.is_bypassed();
        let service = self.get_service()?;
        service.handle_shutdown(req, bypassed).await
    }
    
    async fn propose_conf_change(
        &self,
        req: ProposeConfChangeRequest,
        meta: Metadata,
    ) -> Result<ProposeConfChangeResponse, CurpError> {
        let bypassed = meta.is_bypassed();
        let service = self.get_service()?;
        service.handle_propose_conf_change(req, bypassed).await
    }
    
    fn publish(
        &self,
        req: PublishRequest,
        meta: Metadata,
    ) -> Result<PublishResponse, CurpError> {
        let bypassed = meta.is_bypassed();
        let service = self.get_service()?;
        service.handle_publish(req, bypassed)
    }
    
    fn fetch_cluster(
        &self,
        req: FetchClusterRequest,
    ) -> Result<FetchClusterResponse, CurpError> {
        let service = self.get_service()?;
        service.handle_fetch_cluster(req)
    }
    
    fn fetch_read_state(
        &self,
        req: FetchReadStateRequest,
    ) -> Result<FetchReadStateResponse, CurpError> {
        let service = self.get_service()?;
        service.handle_fetch_read_state(req)
    }
    
    async fn move_leader(
        &self,
        req: MoveLeaderRequest,
    ) -> Result<MoveLeaderResponse, CurpError> {
        let service = self.get_service()?;
        service.handle_move_leader(req).await
    }
    
    async fn lease_keep_alive(
        &self,
        stream: Box<dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin>,
    ) -> Result<LeaseKeepAliveMsg, CurpError> {
        let service = self.get_service()?;
        service.handle_lease_keep_alive(stream).await
    }
}

#[async_trait]
impl InnerCurpService for CurpProtocolAdapter {
    fn append_entries(
        &self,
        req: AppendEntriesRequest,
    ) -> Result<AppendEntriesResponse, CurpError> {
        let service = self.get_service()?;
        service.handle_append_entries(req)
    }
    
    fn vote(&self, req: VoteRequest) -> Result<VoteResponse, CurpError> {
        let service = self.get_service()?;
        service.handle_vote(req)
    }
    
    async fn install_snapshot(
        &self,
        stream: Box<dyn Stream<Item = Result<InstallSnapshotRequest, CurpError>> + Send + Unpin>,
    ) -> Result<InstallSnapshotResponse, CurpError> {
        let service = self.get_service()?;
        service.handle_install_snapshot(stream).await
    }
    
    fn trigger_shutdown(&self) -> Result<(), CurpError> {
        let service = self.get_service()?;
        service.handle_trigger_shutdown()
    }
    
    async fn try_become_leader_now(&self) -> Result<(), CurpError> {
        let service = self.get_service()?;
        service.handle_try_become_leader_now().await
    }
}