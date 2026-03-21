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
use xlinerpc::status::Status;
use tracing::debug;
use xlineapi::command::Command;

use super::xline_server::CurpServer;
use crate::{router::endpoint::EndPoint as RouterEndpoint, storage::AuthStore};

/// Build transport-agnostic `Metadata` from `tonic::metadata::MetadataMap`
pub(crate) fn metadata_from_tonic(map: &tonic::metadata::MetadataMap) -> Metadata {
    let pairs = map
        .iter()
        .filter_map(|(key, val)| {
            String::from_utf8(val.to_vec())
                .ok()
                .map(|v| (key.to_owned(), v))
        })
        .collect();
    Metadata::from_pairs(pairs)
}

/// Convert `CurpError` → `tonic::Status` via `xlinerpc::Status`
pub(crate) fn curp_error_to_tonic_status(err: CurpError) -> Status {
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


impl Protocol for AuthWrapper {
    type ProposeStreamStream = Pin<Box<dyn Stream<Item = Result<OpResponse, Status>> + Send>>;

    async fn propose_stream(
        &self,
        request: xlinerpc::Request<ProposeRequest>,
    ) -> Result<xlinerpc::Response<Self::ProposeStreamStream>, Status> {
        debug!(
            "AuthWrapper (xlinerpc) received propose request: {}",
            request.get_ref().propose_id()
        );
        // Try full xlinerpc auth (token + mTLS peer certs)
        let mut req = request.get_ref().clone();
    
        let auth_info = if self.auth_store.is_enabled() {
            if let Some(token) = get_token(request.metadata()) {
                Some(self.auth_store.verify(&token)?)
            } else {
                if let Some(cn) = get_cn_from_xlinerpc_request(&request) {
                    Some(AuthInfo {
                        username: cn,
                        auth_revision: self.auth_store.revision(),
                    })
                } else {
                    None
                }
            }
        } else {
            None
        };
    
        if let Some(auth_info) = auth_info {
            let mut command: Command = req.cmd().map_err(|e| Status::internal(e.to_string()))?;
            command.set_auth_info(auth_info);
            req.command = command.encode();
        }
    
        let meta = metadata_from_xlinerpc(request.meta());
        let stream = CurpService::propose_stream(&self.curp_server, req, meta)
            .await
            .map_err(curp_error_to_xlinerpc_status)?;
        let mapped = stream.map(|r| r.map_err(curp_error_to_xlinerpc_status));
        Ok(xlinerpc::Response::from_data(Box::pin(mapped)))
    }

    // Helper function: Get CN from xlinerpc::Request
    fn get_cn_from_xlinerpc_request<T>(request: &xlinerpc::Request<T>) -> Option<String> {
        if let Some(tonic_request) = request.inner_tonic_request() {
            get_cn(tonic_request)
        } else {
            None
        }
    }

    async fn record(
        &self,
        request: xlinerpc::Request<RecordRequest>,
    ) -> Result<xlinerpc::Response<RecordResponse>, Status> {
        let meta = metadata_from_xlinerpc(request.meta());
        Ok(xlinerpc::Response::from_data(
            CurpService::record(self, request.into_inner(), meta)
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn read_index(
        &self,
        request: xlinerpc::Request<ReadIndexRequest>,
    ) -> Result<xlinerpc::Response<ReadIndexResponse>, Status> {
        let meta = metadata_from_xlinerpc(request.meta());
        Ok(xlinerpc::Response::from_data(
            CurpService::read_index(self, meta).map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn shutdown(
        &self,
        request: xlinerpc::Request<ShutdownRequest>,
    ) -> Result<xlinerpc::Response<ShutdownResponse>, Status> {
        let meta = metadata_from_xlinerpc(request.meta());
        Ok(xlinerpc::Response::from_data(
            CurpService::shutdown(self, request.into_inner(), meta)
                .await
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn propose_conf_change(
        &self,
        request: xlinerpc::Request<ProposeConfChangeRequest>,
    ) -> Result<xlinerpc::Response<ProposeConfChangeResponse>, Status> {
        let meta = metadata_from_xlinerpc(request.meta());
        Ok(xlinerpc::Response::from_data(
            CurpService::propose_conf_change(self, request.into_inner(), meta)
                .await
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn publish(
        &self,
        request: xlinerpc::Request<PublishRequest>,
    ) -> Result<xlinerpc::Response<PublishResponse>, Status> {
        let meta = metadata_from_xlinerpc(request.meta());
        Ok(xlinerpc::Response::from_data(
            CurpService::publish(self, request.into_inner(), meta)
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn fetch_cluster(
        &self,
        request: xlinerpc::Request<FetchClusterRequest>,
    ) -> Result<xlinerpc::Response<FetchClusterResponse>, Status> {
        Ok(xlinerpc::Response::from_data(
            CurpService::fetch_cluster(self, request.into_inner())
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn fetch_read_state(
        &self,
        request: xlinerpc::Request<FetchReadStateRequest>,
    ) -> Result<xlinerpc::Response<FetchReadStateResponse>, Status> {
        Ok(xlinerpc::Response::from_data(
            CurpService::fetch_read_state(self, request.into_inner())
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn move_leader(
        &self,
        request: xlinerpc::Request<MoveLeaderRequest>,
    ) -> Result<xlinerpc::Response<MoveLeaderResponse>, Status> {
        Ok(xlinerpc::Response::from_data(
            CurpService::move_leader(self, request.into_inner())
                .await
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn lease_keep_alive(
        &self,
        request: xlinerpc::Request<impl Stream<Item = Result<LeaseKeepAliveMsg, Status>> + Send>,
    ) -> Result<xlinerpc::Response<LeaseKeepAliveMsg>, Status> {
        let stream = request.into_inner();
        let curp_stream: Box<
            dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin,
        > = Box::new(stream.map(|r| r.map_err(CurpError::from)));
        Ok(xlinerpc::Response::from_data(
            CurpService::lease_keep_alive(self, curp_stream)
                .await
                .map_err(curp_error_to_tonic_status)?,
        ))
    }
}

pub(crate) struct Server {
    server: Arc<AuthWrapper>,
}
impl Server {
    #[allow(unused)]
    pub(crate) fn new(server: AuthWrapper) -> Self {
        Self {
            server: Arc::new(server),
        }
    }
    #[allow(unused)]
    pub(crate) fn from_arc(server: Arc<AuthWrapper>) -> Self {
        Self { server: server }
    }
    pub(crate) fn endpoint(self) -> RouterEndpoint<Arc<AuthWrapper>> {
        RouterEndpoint::new(self.server)
            .add_server_streaming_fn(
                "/ProposeStream",
                move |this: Arc<AuthWrapper>, request: tonic::Request<ProposeRequest>| async move {
                    Protocol::propose_stream(&*this, request).await
                },
            )
            .add_unary_fn(
                "/Record",
                move |this: Arc<AuthWrapper>, request: tonic::Request<RecordRequest>| async move {
                    Protocol::record(&*this, request).await
                },
            )
            .add_unary_fn(
                "/ReadIndex",
                move |this: Arc<AuthWrapper>, request: tonic::Request<ReadIndexRequest>| async move {
                    Protocol::read_index(&*this, request).await
                },
            )
            .add_unary_fn(
                "/ProposeConfChange",
                move |this: Arc<AuthWrapper>, request: tonic::Request<ProposeConfChangeRequest>| async move {
                    Protocol::propose_conf_change(&*this, request).await
                },
            )
            .add_unary_fn(
                "/Publish",
                move |this: Arc<AuthWrapper>, request: tonic::Request<PublishRequest>| async move {
                    Protocol::publish(&*this, request).await
                }
            )
            .add_unary_fn(
                "/Shutdown",
                move |this: Arc<AuthWrapper>, request: tonic::Request<ShutdownRequest>| async move {
                    Protocol::shutdown(&*this, request).await
                },
            )
            .add_unary_fn(
                "/FetchCluster",
                move |this: Arc<AuthWrapper>, request: tonic::Request<FetchClusterRequest>| async move {
                    Protocol::fetch_cluster(&*this, request).await
                },
            )
            .add_unary_fn(
                "/FetchReadState",
                move |this: Arc<AuthWrapper>, request: tonic::Request<FetchReadStateRequest>| async move {
                    Protocol::fetch_read_state(&*this, request).await
                },
            )
            .add_unary_fn(
                "/MoveLeader",
                move |this: Arc<AuthWrapper>, request: tonic::Request<MoveLeaderRequest>| async move {
                    Protocol::move_leader(&*this, request).await
                },
            )
            .add_client_streaming_fn(
                "/LeaseKeepAlive",
                move |this: Arc<AuthWrapper>, request: tonic::Request<tonic::Streaming<LeaseKeepAliveMsg>>| async move {
                    Protocol::lease_keep_alive(&*this, request).await
                },
            )
    }
}
