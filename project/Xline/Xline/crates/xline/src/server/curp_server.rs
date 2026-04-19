use std::sync::Arc;

use crate::server::auth_wrapper::metadata_from_rpc;
use curp::rpc::{
    CurpError, CurpService, FetchClusterRequest, FetchReadStateRequest, LeaseKeepAliveMsg,
    MoveLeaderRequest, ProposeConfChangeRequest, ProposeRequest, PublishRequest, ReadIndexRequest,
    RecordRequest, ShutdownRequest,
};
use futures::{Stream, StreamExt};
use xlinerpc::Status;
use xlinerpc::server::EndPoint as RouterEndpoint;

#[allow(unused)]
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
    #[allow(unused)]
    pub(crate) fn endpoint(self) -> RouterEndpoint<Arc<T>> {
        RouterEndpoint::new(self.server)
            .add_server_streaming_fn(
                "/ProposeStream",
                move |this: Arc<T>, request: xlinerpc::Request<ProposeRequest>| async move {
                    let req = request.get_ref().clone();
                    let meta = metadata_from_rpc(request.metadata());
                    let stream = CurpService::propose_stream(&*this, req, meta)
                        .await
                        .map_err(Status::from)?;
                    let mapped = stream.map(|r| r.map_err(Status::from));
                    Ok(xlinerpc::Response::from_data(Box::pin(mapped)))
                },
            )
            .add_unary_fn(
                "/Record",
                move |this: Arc<T>, request: xlinerpc::Request<RecordRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    Ok(xlinerpc::Response::from_data(
                        CurpService::record(&*this, request.into_inner(), meta)
                            .map_err(Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/ReadIndex",
                move |this: Arc<T>, request: xlinerpc::Request<ReadIndexRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    Ok(xlinerpc::Response::from_data(
                        CurpService::read_index(&*this, meta)
                            .map_err(Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/ProposeConfChange",
                move |this: Arc<T>, request: xlinerpc::Request<ProposeConfChangeRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    Ok(xlinerpc::Response::from_data(
                        CurpService::propose_conf_change(&*this, request.into_inner(), meta)
                            .await
                            .map_err(Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/Publish",
                move |this: Arc<T>, request: xlinerpc::Request<PublishRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    Ok(xlinerpc::Response::from_data(
                        CurpService::publish(&*this, request.into_inner(), meta)
                            .map_err(Status::from)?,
                    ))
                }
            )
            .add_unary_fn(
                "/Shutdown",
                move |this: Arc<T>, request: xlinerpc::Request<ShutdownRequest>| async move {
                    let meta = metadata_from_rpc(request.metadata());
                    Ok(xlinerpc::Response::from_data(
                        CurpService::shutdown(&*this, request.into_inner(), meta)
                            .await
                            .map_err(Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/FetchCluster",
                move |this: Arc<T>, request: xlinerpc::Request<FetchClusterRequest>| async move {
                    Ok(xlinerpc::Response::from_data(
                        CurpService::fetch_cluster(&*this, request.into_inner())
                            .map_err(Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/FetchReadState",
                move |this: Arc<T>, request: xlinerpc::Request<FetchReadStateRequest>| async move {
                    Ok(xlinerpc::Response::from_data(
                        CurpService::fetch_read_state(&*this, request.into_inner())
                            .map_err(Status::from)?,
                    ))
                },
            )
            .add_unary_fn(
                "/MoveLeader",
                move |this: Arc<T>, request: xlinerpc::Request<MoveLeaderRequest>| async move {
                    Ok(xlinerpc::Response::from_data(
                        CurpService::move_leader(&*this, request.into_inner())
                            .await
                            .map_err(Status::from)?
                    ))
                },
            )
            .add_client_streaming_fn(
                "/LeaseKeepAlive",
                move |this: Arc<T>, request: xlinerpc::Request<xlinerpc::Streaming<LeaseKeepAliveMsg>>| async move {
                    let stream = request.into_inner();
                    let curp_stream: Box<
                        dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin,
                    > = Box::new(stream.map(|r| r.map_err(CurpError::from)));
                    Ok(xlinerpc::Response::from_data(
                        CurpService::lease_keep_alive(&*this, curp_stream)
                            .await
                            .map_err(Status::from)?,
                    ))
                },
            )
    }
}
