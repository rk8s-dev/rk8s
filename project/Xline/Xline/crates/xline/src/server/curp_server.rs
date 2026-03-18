use std::sync::Arc;

use curp::rpc::{
        FetchClusterRequest, FetchReadStateRequest, ProposeConfChangeRequest, ProposeRequest, Protocol, PublishRequest, ReadIndexRequest, RecordRequest, ShutdownRequest,
        MoveLeaderRequest, LeaseKeepAliveMsg
    };
use crate::router::endpoint::EndPoint as RouterEndpoint;

pub(crate) struct Server<T> {
    server: Arc<T>,
}
impl<T> Server<T>
where
    T: Protocol
{
    #[allow(unused)]
    pub(crate) fn new(server: T) -> Self {
        Self {
            server: Arc::new(server),
        }
    }
    #[allow(unused)]
    pub(crate) fn from_arc(server: Arc<T>) -> Self {
        Self {
            server: server,
        }
    }
    pub(crate) fn endpoint(self) -> RouterEndpoint<Arc<T>> {
        RouterEndpoint::new(self.server)
            .add_server_streaming_fn(
                "/ProposeStream",
                move |this: Arc<T>, request: tonic::Request<ProposeRequest>| async move {
                    this.propose_stream(request).await
                },
            )
            .add_unary_fn(
                "/Record",
                move |this: Arc<T>, request: tonic::Request<RecordRequest>| async move {
                    this.record(request).await
                },
            )
            .add_unary_fn(
                "/ReadIndex",
                move |this: Arc<T>, request: tonic::Request<ReadIndexRequest>| async move {
                    this.read_index(request).await
                },
            )
            .add_unary_fn(
                "/ProposeConfChange",
                move |this: Arc<T>, request: tonic::Request<ProposeConfChangeRequest>| async move {
                    this.propose_conf_change(request).await
                },
            )
            .add_unary_fn(
                "/Publish",
                move |this: Arc<T>, request: tonic::Request<PublishRequest>| async move {
                    this.publish(request).await
                }
            )
            .add_unary_fn(
                "/Shutdown",
                move |this: Arc<T>, request: tonic::Request<ShutdownRequest>| async move {
                    this.shutdown(request).await
                },
            )
            .add_unary_fn(
                "/FetchCluster",
                move |this: Arc<T>, request: tonic::Request<FetchClusterRequest>| async move {
                    this.fetch_cluster(request).await
                },
            )
            .add_unary_fn(
                "/FetchReadState",
                move |this: Arc<T>, request: tonic::Request<FetchReadStateRequest>| async move {
                    this.fetch_read_state(request).await
                },
            )
            .add_unary_fn(
                "/MoveLeader",
                move |this: Arc<T>, request: tonic::Request<MoveLeaderRequest>| async move {
                    this.move_leader(request).await
                },
            )
            .add_client_streaming_fn(
                "/LeaseKeepAlive",
                move |this: Arc<T>, request: tonic::Request<tonic::Streaming<LeaseKeepAliveMsg>>| async move {
                    this.lease_keep_alive(request).await
                },
            )
    }
}
