use std::{pin::Pin, sync::Arc};

#[allow(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    unused_qualifications,
    unreachable_pub,
    variant_size_differences,
    missing_copy_implementations,
    missing_docs,
    trivial_casts,
    unused_results
)]
pub(crate) mod commandpb {
    pub mod protocol_server {
        #![allow(
            unused_variables,
            dead_code,
            missing_docs,
            clippy::wildcard_imports,
            clippy::let_unit_value
        )]
        use futures::Stream;
        use xlinerpc::{Request, Response, Status};
        /// Generated trait containing gRPC methods that should be implemented for
        /// use with ProtocolServer.
        #[async_trait]
        pub trait Protocol: std::marker::Send + std::marker::Sync + 'static {
            /// Server streaming response type for the ProposeStream method.
            type ProposeStreamStream: Stream<
                    Item = std::result::Result<::curp::rpc::OpResponse, Status>,
                > + std::marker::Send
                + 'static;
            /// Unary
            async fn propose_stream(
                &self,
                request: Request<::curp::rpc::ProposeRequest>,
            ) -> std::result::Result<Response<Self::ProposeStreamStream>, Status>;
            async fn record(
                &self,
                request: Request<::curp::rpc::RecordRequest>,
            ) -> std::result::Result<Response<::curp::rpc::RecordResponse>, Status>;
            async fn read_index(
                &self,
                request: Request<::curp::rpc::ReadIndexRequest>,
            ) -> std::result::Result<Response<::curp::rpc::ReadIndexResponse>, Status>;
            async fn propose_conf_change(
                &self,
                request: Request<::curp::rpc::ProposeConfChangeRequest>,
            ) -> std::result::Result<
                Response<::curp::rpc::ProposeConfChangeResponse>,
                Status,
            >;
            async fn publish(
                &self,
                request: Request<::curp::rpc::PublishRequest>,
            ) -> std::result::Result<Response<::curp::rpc::PublishResponse>, Status>;
            async fn shutdown(
                &self,
                request: Request<::curp::rpc::ShutdownRequest>,
            ) -> std::result::Result<Response<::curp::rpc::ShutdownResponse>, Status>;
            async fn fetch_cluster(
                &self,
                request: Request<::curp::rpc::FetchClusterRequest>,
            ) -> std::result::Result<
                Response<::curp::rpc::FetchClusterResponse>,
                Status,
            >;
            async fn fetch_read_state(
                &self,
                request: Request<::curp::rpc::FetchReadStateRequest>,
            ) -> std::result::Result<
                Response<::curp::rpc::FetchReadStateResponse>,
                Status,
            >;
            async fn move_leader(
                &self,
                request: Request<::curp::rpc::MoveLeaderRequest>,
            ) -> std::result::Result<Response<::curp::rpc::MoveLeaderResponse>, Status>;
            /// Stream
            async fn lease_keep_alive(
                &self,
                request: Request<impl Stream<Item = std::result::Result<::curp::rpc::LeaseKeepAliveMsg, Status>> + Send>,
            ) -> std::result::Result<Response<::curp::rpc::LeaseKeepAliveMsg>, Status>;
        }
    }
}

use crate::server::auth_wrapper::commandpb::protocol_server::Protocol;
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
use xlinerpc::{Request, Response, Status, MetaData};
use tracing::debug;
use xlineapi::command::Command;

use super::xline_server::CurpServer;
use crate::{router::endpoint::EndPoint as RouterEndpoint, storage::AuthStore};

/// Build transport-agnostic `Metadata` from `xlinerpc::MetaData`
pub(crate) fn metadata_from_xlinerpc(meta: &MetaData) -> Metadata {
    let pairs = meta
        .iter()
        .filter_map(|(key, val)| {
            String::from_utf8(val.to_vec())
                .ok()
                .and_then(|v| String::from_utf8(key.to_vec()).ok())
                .map(|k| (k, v))
        })
        .collect();
    Metadata::from_pairs(pairs)
}

/// Convert `CurpError` → `xlinerpc::Status`
pub(crate) fn curp_error_to_xlinerpc_status(err: CurpError) -> Status {
    err.into()
}

/// Get token from metadata
fn get_token(meta: &MetaData) -> Option<&str> {
    // First try to get token from authorization header (standard HTTP auth)
    if let Some(token) = meta.get_str("authorization")
        .and_then(|result| match result {
            Ok(token) => Some(token),
            Err(e) => {
                debug!("Failed to decode authorization token: {}", e);
                None
            }
        })
        .and_then(|s| match s.strip_prefix("Bearer ") {
            Some(token) => Some(token),
            None => {
                debug!("Authorization token missing 'Bearer ' prefix");
                None
            }
        }) {
        return Some(token);
    }
    
    // Then try to get token from CURP-specific token header
    if let Some(token) = meta.get_str("token")
        .and_then(|result| match result {
            Ok(token) => Some(token),
            Err(e) => {
                debug!("Failed to decode CURP token: {}", e);
                None
            }
        }) {
        return Some(token);
    }
    
    None
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
        request: Request<ProposeRequest>,
    ) -> Result<Response<Self::ProposeStreamStream>, Status> {
        debug!(
            "AuthWrapper (xlinerpc) received propose request: {}",
            request.data().propose_id()
        );
        // Try full xlinerpc auth (token + mTLS peer certs)
        let mut req = request.data().clone();
    
        let auth_info = if self.auth_store.is_enabled() {
            if let Some(token) = get_token(request.meta()) {
                Some(self.auth_store.verify(&token)?)
            } else {
                // TODO: Implement mTLS support for xlinerpc
                None
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
        Ok(Response::from_data(Box::pin(mapped)))
    }

    async fn record(
        &self,
        request: Request<RecordRequest>,
    ) -> Result<Response<RecordResponse>, Status> {
        let meta = metadata_from_xlinerpc(request.meta());
        Ok(Response::from_data(
            CurpService::record(self, request.into_data(), meta)
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn read_index(
        &self,
        request: Request<ReadIndexRequest>,
    ) -> Result<Response<ReadIndexResponse>, Status> {
        let meta = metadata_from_xlinerpc(request.meta());
        Ok(Response::from_data(
            CurpService::read_index(self, meta).map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn shutdown(
        &self,
        request: Request<ShutdownRequest>,
    ) -> Result<Response<ShutdownResponse>, Status> {
        let meta = metadata_from_xlinerpc(request.meta());
        Ok(Response::from_data(
            CurpService::shutdown(self, request.into_data(), meta)
                .await
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn propose_conf_change(
        &self,
        request: Request<ProposeConfChangeRequest>,
    ) -> Result<Response<ProposeConfChangeResponse>, Status> {
        let meta = metadata_from_xlinerpc(request.meta());
        Ok(Response::from_data(
            CurpService::propose_conf_change(self, request.into_data(), meta)
                .await
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn publish(
        &self,
        request: Request<PublishRequest>,
    ) -> Result<Response<PublishResponse>, Status> {
        let meta = metadata_from_xlinerpc(request.meta());
        Ok(Response::from_data(
            CurpService::publish(self, request.into_data(), meta)
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn fetch_cluster(
        &self,
        request: Request<FetchClusterRequest>,
    ) -> Result<Response<FetchClusterResponse>, Status> {
        Ok(Response::from_data(
            CurpService::fetch_cluster(self, request.into_data())
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn fetch_read_state(
        &self,
        request: Request<FetchReadStateRequest>,
    ) -> Result<Response<FetchReadStateResponse>, Status> {
        Ok(Response::from_data(
            CurpService::fetch_read_state(self, request.into_data())
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn move_leader(
        &self,
        request: Request<MoveLeaderRequest>,
    ) -> Result<Response<MoveLeaderResponse>, Status> {
        Ok(Response::from_data(
            CurpService::move_leader(self, request.into_data())
                .await
                .map_err(curp_error_to_xlinerpc_status)?,
        ))
    }

    async fn lease_keep_alive(
        &self,
        request: Request<impl Stream<Item = Result<LeaseKeepAliveMsg, Status>> + Send>,
    ) -> Result<Response<LeaseKeepAliveMsg>, Status> {
        let stream = request.into_data();
        let curp_stream: Box<
            dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin,
        > = Box::new(stream.map(|r| r.map_err(CurpError::from)));
        Ok(Response::from_data(
            CurpService::lease_keep_alive(self, curp_stream)
                .await
                .map_err(curp_error_to_xlinerpc_status)?,
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
                move |this: Arc<AuthWrapper>, request: Request<ProposeRequest>| async move {
                    Protocol::propose_stream(&*this, request).await
                },
            )
            .add_unary_fn(
                "/Record",
                move |this: Arc<AuthWrapper>, request: Request<RecordRequest>| async move {
                    Protocol::record(&*this, request).await
                },
            )
            .add_unary_fn(
                "/ReadIndex",
                move |this: Arc<AuthWrapper>, request: Request<ReadIndexRequest>| async move {
                    Protocol::read_index(&*this, request).await
                },
            )
            .add_unary_fn(
                "/ProposeConfChange",
                move |this: Arc<AuthWrapper>, request: Request<ProposeConfChangeRequest>| async move {
                    Protocol::propose_conf_change(&*this, request).await
                },
            )
            .add_unary_fn(
                "/Publish",
                move |this: Arc<AuthWrapper>, request: Request<PublishRequest>| async move {
                    Protocol::publish(&*this, request).await
                }
            )
            .add_unary_fn(
                "/Shutdown",
                move |this: Arc<AuthWrapper>, request: Request<ShutdownRequest>| async move {
                    Protocol::shutdown(&*this, request).await
                },
            )
            .add_unary_fn(
                "/FetchCluster",
                move |this: Arc<AuthWrapper>, request: Request<FetchClusterRequest>| async move {
                    Protocol::fetch_cluster(&*this, request).await
                },
            )
            .add_unary_fn(
                "/FetchReadState",
                move |this: Arc<AuthWrapper>, request: Request<FetchReadStateRequest>| async move {
                    Protocol::fetch_read_state(&*this, request).await
                },
            )
            .add_unary_fn(
                "/MoveLeader",
                move |this: Arc<AuthWrapper>, request: Request<MoveLeaderRequest>| async move {
                    Protocol::move_leader(&*this, request).await
                },
            )
            .add_client_streaming_fn(
                "/LeaseKeepAlive",
                move |this: Arc<AuthWrapper>, request: Request<impl Stream<Item = Result<LeaseKeepAliveMsg, Status>> + Send>| async move {
                    Protocol::lease_keep_alive(&*this, request).await
                },
            )
    }
}