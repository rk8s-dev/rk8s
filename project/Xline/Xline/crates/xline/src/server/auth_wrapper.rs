use std::{pin::Pin, sync::Arc};

use crate::curp_proto::commandpb::protocol_server::Protocol;
use async_trait::async_trait;
use curp::{
    cmd::PbCodec,
    rpc::{
        CurpError, CurpService, FetchClusterRequest, FetchClusterResponse, FetchReadStateRequest,
        FetchReadStateResponse, LeaseKeepAliveMsg, Metadata, MoveLeaderRequest, MoveLeaderResponse,
        OpResponse, ProposeConfChangeRequest, ProposeConfChangeResponse, ProposeRequest,
        PublishRequest, PublishResponse, ReadIndexRequest, ReadIndexResponse, RecordRequest,
        RecordResponse, ShutdownRequest, ShutdownResponse,
    },
};
use futures::{Stream, StreamExt};
use tonic::Status;
use tracing::debug;
use xlineapi::command::Command;

use super::xline_server::CurpServer;
use crate::storage::AuthStore;

/// Build transport-agnostic `Metadata` from `tonic::metadata::MetadataMap`
fn metadata_from_tonic(map: &tonic::metadata::MetadataMap) -> Metadata {
    let pairs = map
        .iter()
        .filter_map(|kv| match kv {
            tonic::metadata::KeyAndValueRef::Ascii(key, val) => val
                .to_str()
                .ok()
                .map(|v| (key.as_str().to_owned(), v.to_owned())),
            _ => None,
        })
        .collect();
    Metadata::from_pairs(pairs)
}

/// Convert `CurpError` → `tonic::Status` via `xlinerpc::Status`
fn curp_error_to_tonic_status(err: CurpError) -> Status {
    let xlinerpc_status: xlinerpc::status::Status = err.into();
    let code = tonic::Code::from(i32::from(xlinerpc_status.code()));
    let details = xlinerpc_status.details();
    if details.is_empty() {
        Status::new(code, xlinerpc_status.message())
    } else {
        Status::with_details(
            code,
            xlinerpc_status.message(),
            bytes::Bytes::copy_from_slice(details),
        )
    }
}

/// Auth wrapper
#[derive(Clone)]
pub(crate) struct AuthWrapper {
    /// Curp server
    curp_server: CurpServer,
    /// Auth store
    auth_store: Arc<AuthStore>,
}

impl AuthWrapper {
    /// Create a new auth wrapper
    pub(crate) fn new(curp_server: CurpServer, auth_store: Arc<AuthStore>) -> Self {
        Self {
            curp_server,
            auth_store,
        }
    }

    /// Inject auth info into a propose request if auth is enabled.
    ///
    /// Extracts token from metadata, verifies it, and sets auth info on the command.
    fn inject_auth_from_token(
        &self,
        req: &mut ProposeRequest,
        token: Option<&str>,
    ) -> Result<(), CurpError> {
        if let Some(auth_info) = self
            .auth_store
            .try_get_auth_info_from_token(token)
            .map_err(CurpError::from)?
        {
            let mut command: Command = req.cmd().map_err(CurpError::from)?;
            command.set_auth_info(auth_info);
            req.command = command.encode();
        }
        Ok(())
    }
}

// ============================================================================
// CurpService implementation (primary, transport-agnostic)
// ============================================================================

#[async_trait]
impl CurpService for AuthWrapper {
    async fn propose_stream(
        &self,
        mut req: ProposeRequest,
        meta: Metadata,
    ) -> Result<Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send + Unpin>, CurpError>
    {
        debug!("AuthWrapper received propose request: {}", req.propose_id());
        self.inject_auth_from_token(&mut req, meta.token())?;
        CurpService::propose_stream(&self.curp_server, req, meta).await
    }

    fn record(&self, req: RecordRequest, meta: Metadata) -> Result<RecordResponse, CurpError> {
        CurpService::record(&self.curp_server, req, meta)
    }

    fn read_index(&self, meta: Metadata) -> Result<ReadIndexResponse, CurpError> {
        CurpService::read_index(&self.curp_server, meta)
    }

    async fn shutdown(
        &self,
        req: ShutdownRequest,
        meta: Metadata,
    ) -> Result<ShutdownResponse, CurpError> {
        CurpService::shutdown(&self.curp_server, req, meta).await
    }

    async fn propose_conf_change(
        &self,
        req: ProposeConfChangeRequest,
        meta: Metadata,
    ) -> Result<ProposeConfChangeResponse, CurpError> {
        CurpService::propose_conf_change(&self.curp_server, req, meta).await
    }

    fn publish(&self, req: PublishRequest, meta: Metadata) -> Result<PublishResponse, CurpError> {
        CurpService::publish(&self.curp_server, req, meta)
    }

    fn fetch_cluster(&self, req: FetchClusterRequest) -> Result<FetchClusterResponse, CurpError> {
        CurpService::fetch_cluster(&self.curp_server, req)
    }

    fn fetch_read_state(
        &self,
        req: FetchReadStateRequest,
    ) -> Result<FetchReadStateResponse, CurpError> {
        CurpService::fetch_read_state(&self.curp_server, req)
    }

    async fn move_leader(&self, req: MoveLeaderRequest) -> Result<MoveLeaderResponse, CurpError> {
        CurpService::move_leader(&self.curp_server, req).await
    }

    async fn lease_keep_alive(
        &self,
        stream: Box<dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin>,
    ) -> Result<LeaseKeepAliveMsg, CurpError> {
        CurpService::lease_keep_alive(&self.curp_server, stream).await
    }
}

// ============================================================================
// Tonic Protocol adapter (xline gRPC boundary layer)
//
// Delegates to CurpService after converting tonic::Request → Metadata.
// For propose_stream, also handles mTLS peer cert auth (get_cn) which is
// only available through tonic::Request.
// ============================================================================

#[tonic::async_trait]
impl Protocol for AuthWrapper {
    type ProposeStreamStream = Pin<Box<dyn Stream<Item = Result<OpResponse, Status>> + Send>>;

    async fn propose_stream(
        &self,
        request: tonic::Request<ProposeRequest>,
    ) -> Result<tonic::Response<Self::ProposeStreamStream>, Status> {
        debug!(
            "AuthWrapper (tonic) received propose request: {}",
            request.get_ref().propose_id()
        );
        // Try full tonic auth (token + mTLS peer certs)
        let mut req = request.get_ref().clone();
        if let Some(auth_info) = self.auth_store.try_get_auth_info_from_request(&request)? {
            let mut command: Command = req.cmd().map_err(|e| Status::internal(e.to_string()))?;
            command.set_auth_info(auth_info);
            req.command = command.encode();
        }
        let meta = metadata_from_tonic(request.metadata());
        let stream = CurpService::propose_stream(&self.curp_server, req, meta)
            .await
            .map_err(curp_error_to_tonic_status)?;
        let mapped = stream.map(|r| r.map_err(curp_error_to_tonic_status));
        Ok(tonic::Response::new(Box::pin(mapped)))
    }

    async fn record(
        &self,
        request: tonic::Request<RecordRequest>,
    ) -> Result<tonic::Response<RecordResponse>, Status> {
        let meta = metadata_from_tonic(request.metadata());
        Ok(tonic::Response::new(
            CurpService::record(self, request.into_inner(), meta)
                .map_err(curp_error_to_tonic_status)?,
        ))
    }

    async fn read_index(
        &self,
        request: tonic::Request<ReadIndexRequest>,
    ) -> Result<tonic::Response<ReadIndexResponse>, Status> {
        let meta = metadata_from_tonic(request.metadata());
        Ok(tonic::Response::new(
            CurpService::read_index(self, meta).map_err(curp_error_to_tonic_status)?,
        ))
    }

    async fn shutdown(
        &self,
        request: tonic::Request<ShutdownRequest>,
    ) -> Result<tonic::Response<ShutdownResponse>, Status> {
        let meta = metadata_from_tonic(request.metadata());
        Ok(tonic::Response::new(
            CurpService::shutdown(self, request.into_inner(), meta)
                .await
                .map_err(curp_error_to_tonic_status)?,
        ))
    }

    async fn propose_conf_change(
        &self,
        request: tonic::Request<ProposeConfChangeRequest>,
    ) -> Result<tonic::Response<ProposeConfChangeResponse>, Status> {
        let meta = metadata_from_tonic(request.metadata());
        Ok(tonic::Response::new(
            CurpService::propose_conf_change(self, request.into_inner(), meta)
                .await
                .map_err(curp_error_to_tonic_status)?,
        ))
    }

    async fn publish(
        &self,
        request: tonic::Request<PublishRequest>,
    ) -> Result<tonic::Response<PublishResponse>, Status> {
        let meta = metadata_from_tonic(request.metadata());
        Ok(tonic::Response::new(
            CurpService::publish(self, request.into_inner(), meta)
                .map_err(curp_error_to_tonic_status)?,
        ))
    }

    async fn fetch_cluster(
        &self,
        request: tonic::Request<FetchClusterRequest>,
    ) -> Result<tonic::Response<FetchClusterResponse>, Status> {
        Ok(tonic::Response::new(
            CurpService::fetch_cluster(self, request.into_inner())
                .map_err(curp_error_to_tonic_status)?,
        ))
    }

    async fn fetch_read_state(
        &self,
        request: tonic::Request<FetchReadStateRequest>,
    ) -> Result<tonic::Response<FetchReadStateResponse>, Status> {
        Ok(tonic::Response::new(
            CurpService::fetch_read_state(self, request.into_inner())
                .map_err(curp_error_to_tonic_status)?,
        ))
    }

    async fn move_leader(
        &self,
        request: tonic::Request<MoveLeaderRequest>,
    ) -> Result<tonic::Response<MoveLeaderResponse>, Status> {
        Ok(tonic::Response::new(
            CurpService::move_leader(self, request.into_inner())
                .await
                .map_err(curp_error_to_tonic_status)?,
        ))
    }

    async fn lease_keep_alive(
        &self,
        request: tonic::Request<tonic::Streaming<LeaseKeepAliveMsg>>,
    ) -> Result<tonic::Response<LeaseKeepAliveMsg>, Status> {
        let stream = request.into_inner();
        let curp_stream: Box<
            dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin,
        > = Box::new(stream.map(|r| r.map_err(CurpError::from)));
        Ok(tonic::Response::new(
            CurpService::lease_keep_alive(self, curp_stream)
                .await
                .map_err(curp_error_to_tonic_status)?,
        ))
    }
}
