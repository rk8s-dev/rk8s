use std::sync::Arc;

use crate::{
    router::endpoint::EndPoint as RouterEndpoint,
    server::auth_wrapper::{curp_error_to_tonic_status, metadata_from_tonic},
};
use curp::rpc::{
    CurpError, CurpService, FetchClusterRequest, FetchReadStateRequest, LeaseKeepAliveMsg,
    MoveLeaderRequest, ProposeConfChangeRequest, ProposeRequest, PublishRequest, ReadIndexRequest,
    RecordRequest, ShutdownRequest,
};
use futures::{Stream, StreamExt};

pub(crate) struct Server<T> {
    server: Arc<T>,
}
impl<T> Server<T>
where
    T: CurpService,
{
    #[allow(unused)]
    pub(crate) fn new(server: T) -> Self {
        Self {
            server: Arc::new(server),
        }
    }
    #[allow(unused)]
    pub(crate) fn from_arc(server: Arc<T>) -> Self {
        Self { server: server }
    }
    pub(crate) fn endpoint(self) -> RouterEndpoint<Arc<T>> {
        RouterEndpoint::new(self.server)
            .add_server_streaming_fn(
                "/ProposeStream",
                move |this: Arc<T>, request: tonic::Request<ProposeRequest>| async move {
                    let req = request.get_ref().clone();
                    let meta = metadata_from_tonic(request.metadata());
                    let stream = CurpService::propose_stream(&*this, req, meta)
                        .await
                        .map_err(curp_error_to_tonic_status)?;
                    let mapped = stream.map(|r| r.map_err(curp_error_to_tonic_status));
                    Ok(tonic::Response::new(Box::pin(mapped)))
                },
            )
            .add_unary_fn(
                "/Record",
                move |this: Arc<T>, request: tonic::Request<RecordRequest>| async move {
                    let meta = metadata_from_tonic(request.metadata());
                    Ok(tonic::Response::new(
                        CurpService::record(&*this, request.into_inner(), meta)
                            .map_err(curp_error_to_tonic_status)?,
                    ))
                },
            )
            .add_unary_fn(
                "/ReadIndex",
                move |this: Arc<T>, request: tonic::Request<ReadIndexRequest>| async move {
                    let meta = metadata_from_tonic(request.metadata());
                    Ok(tonic::Response::new(
                        CurpService::read_index(&*this, meta)
                            .map_err(curp_error_to_tonic_status)?,
                    ))
                },
            )
            .add_unary_fn(
                "/ProposeConfChange",
                move |this: Arc<T>, request: tonic::Request<ProposeConfChangeRequest>| async move {
                    let meta = metadata_from_tonic(request.metadata());
                    Ok(tonic::Response::new(
                        CurpService::propose_conf_change(&*this, request.into_inner(), meta)
                            .await
                            .map_err(curp_error_to_tonic_status)?,
                    ))
                },
            )
            .add_unary_fn(
                "/Publish",
                move |this: Arc<T>, request: tonic::Request<PublishRequest>| async move {
                    let meta = metadata_from_tonic(request.metadata());
                    Ok(tonic::Response::new(
                        CurpService::publish(&*this, request.into_inner(), meta)
                            .map_err(curp_error_to_tonic_status)?,
                    ))
                }
            )
            .add_unary_fn(
                "/Shutdown",
                move |this: Arc<T>, request: tonic::Request<ShutdownRequest>| async move {
                    let meta = metadata_from_tonic(request.metadata());
                    Ok(tonic::Response::new(
                        CurpService::shutdown(&*this, request.into_inner(), meta)
                            .await
                            .map_err(curp_error_to_tonic_status)?,
                    ))
                },
            )
            .add_unary_fn(
                "/FetchCluster",
                move |this: Arc<T>, request: tonic::Request<FetchClusterRequest>| async move {
                    Ok(tonic::Response::new(
                        CurpService::fetch_cluster(&*this, request.into_inner())
                            .map_err(curp_error_to_tonic_status)?,
                    ))
                },
            )
            .add_unary_fn(
                "/FetchReadState",
                move |this: Arc<T>, request: tonic::Request<FetchReadStateRequest>| async move {
                    Ok(tonic::Response::new(
                        CurpService::fetch_read_state(&*this, request.into_inner())
                            .map_err(curp_error_to_tonic_status)?,
                    ))
                },
            )
            .add_unary_fn(
                "/MoveLeader",
                move |this: Arc<T>, request: tonic::Request<MoveLeaderRequest>| async move {
                    Ok(tonic::Response::new(
                        CurpService::move_leader(&*this, request.into_inner())
                            .await
                            .map_err(curp_error_to_tonic_status)?
                    ))
                },
            )
            .add_client_streaming_fn(
                "/LeaseKeepAlive",
                move |this: Arc<T>, request: tonic::Request<tonic::Streaming<LeaseKeepAliveMsg>>| async move {
                    let stream = request.into_inner();
                    let curp_stream: Box<
                        dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin,
                    > = Box::new(stream.map(|r| r.map_err(CurpError::from)));
                    Ok(tonic::Response::new(
                        CurpService::lease_keep_alive(&*this, curp_stream)
                            .await
                            .map_err(curp_error_to_tonic_status)?,
                    ))
                },
            )
    }
}
