use std::sync::Arc;

use curp::{
    cmd::PbCodec,
    rpc::{
        FetchClusterRequest, FetchClusterResponse, FetchReadStateRequest, FetchReadStateResponse,
        LeaseKeepAliveMsg, MoveLeaderRequest, MoveLeaderResponse, OpResponse,
        ProposeConfChangeRequest, ProposeConfChangeResponse, ProposeRequest, Protocol,
        PublishRequest, PublishResponse, ReadIndexRequest, ReadIndexResponse, RecordRequest,
        RecordResponse, ShutdownRequest, ShutdownResponse,
    },
};
use flume::r#async::RecvStream;
use tonic::Status;
use tracing::debug;
use xlineapi::command::Command;
// TODO: use our own status type
// use xlinerpc::status::Status;
use super::xline_server::CurpServer;
use crate::{
    router::endpoint::EndPoint as RouterEndpoint,
    storage::AuthStore,
};

/// Auth wrapper
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
}

#[tonic::async_trait]
impl Protocol for AuthWrapper {
    type ProposeStreamStream = RecvStream<'static, Result<OpResponse, Status>>;

    async fn propose_stream(
        &self,
        mut request: tonic::Request<ProposeRequest>,
    ) -> Result<tonic::Response<Self::ProposeStreamStream>, Status> {
        debug!(
            "AuthWrapper received propose request: {}",
            request.get_ref().propose_id()
        );
        if let Some(auth_info) = self.auth_store.try_get_auth_info_from_request(&request)? {
            let mut command: Command = request
                .get_ref()
                .cmd()
                .map_err(|e| Status::internal(e.to_string()))?;
            command.set_auth_info(auth_info);
            request.get_mut().command = command.encode();
        }
        self.curp_server.propose_stream(request).await
    }

    async fn record(
        &self,
        request: tonic::Request<RecordRequest>,
    ) -> Result<tonic::Response<RecordResponse>, Status> {
        self.curp_server.record(request).await
    }

    async fn read_index(
        &self,
        request: tonic::Request<ReadIndexRequest>,
    ) -> Result<tonic::Response<ReadIndexResponse>, Status> {
        self.curp_server.read_index(request).await
    }

    async fn shutdown(
        &self,
        request: tonic::Request<ShutdownRequest>,
    ) -> Result<tonic::Response<ShutdownResponse>, Status> {
        self.curp_server.shutdown(request).await
    }

    async fn propose_conf_change(
        &self,
        request: tonic::Request<ProposeConfChangeRequest>,
    ) -> Result<tonic::Response<ProposeConfChangeResponse>, Status> {
        self.curp_server.propose_conf_change(request).await
    }

    async fn publish(
        &self,
        request: tonic::Request<PublishRequest>,
    ) -> Result<tonic::Response<PublishResponse>, Status> {
        self.curp_server.publish(request).await
    }

    async fn fetch_cluster(
        &self,
        request: tonic::Request<FetchClusterRequest>,
    ) -> Result<tonic::Response<FetchClusterResponse>, Status> {
        self.curp_server.fetch_cluster(request).await
    }

    async fn fetch_read_state(
        &self,
        request: tonic::Request<FetchReadStateRequest>,
    ) -> Result<tonic::Response<FetchReadStateResponse>, Status> {
        self.curp_server.fetch_read_state(request).await
    }

    async fn move_leader(
        &self,
        request: tonic::Request<MoveLeaderRequest>,
    ) -> Result<tonic::Response<MoveLeaderResponse>, Status> {
        self.curp_server.move_leader(request).await
    }

    async fn lease_keep_alive(
        &self,
        request: tonic::Request<tonic::Streaming<LeaseKeepAliveMsg>>,
    ) -> Result<tonic::Response<LeaseKeepAliveMsg>, Status> {
        self.curp_server.lease_keep_alive(request).await
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
        Self {
            server: server,
        }
    }
    pub(crate) fn endpoint(self) -> RouterEndpoint<Arc<AuthWrapper>> {
        RouterEndpoint::new(self.server)
            .add_server_streaming_fn(
                "/ProposeStream",
                move |this: Arc<AuthWrapper>, request: tonic::Request<ProposeRequest>| async move {
                    this.propose_stream(request).await
                },
            )
            .add_unary_fn(
                "/Record",
                move |this: Arc<AuthWrapper>, request: tonic::Request<RecordRequest>| async move {
                    this.record(request).await
                },
            )
            .add_unary_fn(
                "/ReadIndex",
                move |this: Arc<AuthWrapper>, request: tonic::Request<ReadIndexRequest>| async move {
                    this.read_index(request).await
                },
            )
            .add_unary_fn(
                "/ProposeConfChange",
                move |this: Arc<AuthWrapper>, request: tonic::Request<ProposeConfChangeRequest>| async move {
                    this.propose_conf_change(request).await
                },
            )
            .add_unary_fn(
                "/Publish",
                move |this: Arc<AuthWrapper>, request: tonic::Request<PublishRequest>| async move {
                    this.publish(request).await
                }
            )
            .add_unary_fn(
                "/Shutdown",
                move |this: Arc<AuthWrapper>, request: tonic::Request<ShutdownRequest>| async move {
                    this.shutdown(request).await
                },
            )
            .add_unary_fn(
                "/FetchCluster",
                move |this: Arc<AuthWrapper>, request: tonic::Request<FetchClusterRequest>| async move {
                    this.fetch_cluster(request).await
                },
            )
            .add_unary_fn(
                "/FetchReadState",
                move |this: Arc<AuthWrapper>, request: tonic::Request<FetchReadStateRequest>| async move {
                    this.fetch_read_state(request).await
                },
            )
            .add_unary_fn(
                "/MoveLeader",
                move |this: Arc<AuthWrapper>, request: tonic::Request<MoveLeaderRequest>| async move {
                    this.move_leader(request).await
                },
            )
            .add_client_streaming_fn(
                "/LeaseKeepAlive",
                move |this: Arc<AuthWrapper>, request: tonic::Request<tonic::Streaming<LeaseKeepAliveMsg>>| async move {
                    this.lease_keep_alive(request).await
                },
            )
    }
}
