use std::sync::Arc;

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
use tracing::debug;
use xlineapi::command::Command;
use xlinerpc::server::EndPoint as RouterEndpoint;

use super::xline_server::CurpServer;
use crate::storage::AuthStore;

/// Build transport-agnostic `Metadata` from `xlinerpc::MetaData`
pub(crate) fn metadata_from_rpc(map: &xlinerpc::MetaData) -> Metadata {
    let pairs = map
        .iter()
        .filter_map(|(k, v)| {
            let key = std::str::from_utf8(k).ok()?;
            let value = std::str::from_utf8(v).ok()?;
            Some((key.to_owned(), value.to_owned()))
        })
        .collect();
    Metadata::from_pairs(pairs)
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
                move |this: Arc<AuthWrapper>, request: xlinerpc::Request<ProposeRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    let mut req = request.into_inner();
                    this.inject_auth_from_token(&mut req, meta.token())?;
                    let stream = CurpService::propose_stream(&*this, req, meta)
                        .await
                        .map_err(xlinerpc::Status::from)?;
                    let mapped = stream.map(|r| r.map_err(xlinerpc::Status::from));
                    Ok(xlinerpc::Response::from_data(Box::pin(mapped)))
                },
            )
            .add_unary_fn(
                "/Record",
                move |this: Arc<AuthWrapper>, request: xlinerpc::Request<RecordRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    Ok(xlinerpc::Response::from_data(
                        CurpService::record(&*this, request.into_inner(), meta)
                            .map_err(xlinerpc::Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/ReadIndex",
                move |this: Arc<AuthWrapper>, request: xlinerpc::Request<ReadIndexRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    Ok(xlinerpc::Response::from_data(
                        CurpService::read_index(&*this, meta).map_err(xlinerpc::Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/ProposeConfChange",
                move |this: Arc<AuthWrapper>, request: xlinerpc::Request<ProposeConfChangeRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    Ok(xlinerpc::Response::from_data(
                        CurpService::propose_conf_change(&*this, request.into_inner(), meta)
                            .await
                            .map_err(xlinerpc::Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/Publish",
                move |this: Arc<AuthWrapper>, request: xlinerpc::Request<PublishRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    Ok(xlinerpc::Response::from_data(
                        CurpService::publish(&*this, request.into_inner(), meta)
                            .map_err(xlinerpc::Status::from)?,
                    ))
                }
            )
            .add_unary_fn(
                "/Shutdown",
                move |this: Arc<AuthWrapper>, request: xlinerpc::Request<ShutdownRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    Ok(xlinerpc::Response::from_data(
                        CurpService::shutdown(&*this, request.into_inner(), meta)
                            .await
                            .map_err(xlinerpc::Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/FetchCluster",
                move |this: Arc<AuthWrapper>, request: xlinerpc::Request<FetchClusterRequest>| async move {
                    Ok(xlinerpc::Response::from_data(
                        CurpService::fetch_cluster(&*this, request.into_inner())
                            .map_err(xlinerpc::Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/FetchReadState",
                move |this: Arc<AuthWrapper>, request: xlinerpc::Request<FetchReadStateRequest>| async move {
                    Ok(xlinerpc::Response::from_data(
                        CurpService::fetch_read_state(&*this, request.into_inner())
                            .map_err(xlinerpc::Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/MoveLeader",
                move |this: Arc<AuthWrapper>, request: xlinerpc::Request<MoveLeaderRequest>| async move {
                    Ok(xlinerpc::Response::from_data(
                        CurpService::move_leader(&*this, request.into_inner())
                            .await
                            .map_err(xlinerpc::Status::from)?,
                    ))
                },
            )
            .add_client_streaming_fn(
                "/LeaseKeepAlive",
                move |this: Arc<AuthWrapper>, request: xlinerpc::Request<xlinerpc::Streaming<LeaseKeepAliveMsg>>| async move {
                    let stream = request.into_inner();
                    let curp_stream: Box<
                        dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin,
                    > = Box::new(stream.map(|r| r.map_err(CurpError::from)));
                    Ok(xlinerpc::Response::from_data(
                        CurpService::lease_keep_alive(&*this, curp_stream)
                            .await
                            .map_err(xlinerpc::Status::from)?,
                    ))
                },
            )
    }
}
