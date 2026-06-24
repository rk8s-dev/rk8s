use std::{pin::Pin, sync::Arc, time::Duration};

use crate::{
    id_gen::IdGenerator,
    metrics,
    rpc::{
        LeaseGrantRequest, LeaseGrantResponse, LeaseKeepAliveRequest, LeaseKeepAliveResponse,
        LeaseLeasesRequest, LeaseLeasesResponse, LeaseRevokeRequest, LeaseRevokeResponse,
        LeaseTimeToLiveRequest, LeaseTimeToLiveResponse, RequestWrapper,
    },
    storage::{AuthStore, LeaseStore},
};
use async_stream::{stream, try_stream};
use clippy_utilities::NumericCast;
use curp::{
    members::ClusterInfo,
    rpc::{DnsFallback, QuicChannel},
};
use futures::{StreamExt, stream::Stream};
use tokio::time;
use tracing::{debug, warn};
use utils::task_manager::{Listener, TaskManager, tasks::TaskName};
use xlineapi::{
    command::{Command, CommandResponse, CurpClient, SyncResponse},
    execute_error::ExecuteError,
};
use xlinerpc::server::EndPoint as RouterEndpoint;
use xlinerpc::{MethodId, Status, Streaming};

/// Default Lease Request Time
const DEFAULT_LEASE_REQUEST_TIME: Duration = Duration::from_millis(500);
/// No transport-level deadline for follower forwarding unless caller supplied one.
const NO_FORWARD_TIMEOUT: Duration = Duration::ZERO;

/// Lease Server
pub(crate) struct LeaseServer {
    /// Lease storage
    lease_storage: Arc<LeaseStore>,
    /// Auth storage
    auth_storage: Arc<AuthStore>,
    /// Consensus client
    client: Arc<CurpClient>,
    /// Id generator
    id_gen: Arc<IdGenerator>,
    /// cluster information
    cluster_info: Arc<ClusterInfo>,
    /// Shared QUIC client for direct RPC forwarding
    quic_client: Arc<dquic::prelude::QuicClient>,
    /// Task manager
    task_manager: Arc<TaskManager>,
}

/// A lease keep alive stream
type KeepAliveStream = Pin<Box<dyn Stream<Item = Result<LeaseKeepAliveResponse, Status>> + Send>>;

impl LeaseServer {
    /// New `LeaseServer`
    pub(crate) fn new(
        lease_storage: Arc<LeaseStore>,
        auth_storage: Arc<AuthStore>,
        client: Arc<CurpClient>,
        id_gen: Arc<IdGenerator>,
        cluster_info: Arc<ClusterInfo>,
        quic_client: Arc<dquic::prelude::QuicClient>,
        task_manager: &Arc<TaskManager>,
    ) -> Arc<Self> {
        let lease_server = Arc::new(Self {
            lease_storage,
            auth_storage,
            client,
            id_gen,
            cluster_info,
            quic_client,
            task_manager: Arc::clone(task_manager),
        });
        task_manager.spawn(TaskName::RevokeExpiredLeases, |n| {
            Self::revoke_expired_leases_task(Arc::clone(&lease_server), n)
        });
        lease_server
    }

    /// Task of revoke expired leases
    #[allow(clippy::arithmetic_side_effects, clippy::ignored_unit_patterns)] // Introduced by tokio::select!
    async fn revoke_expired_leases_task(
        lease_server: Arc<LeaseServer>,
        shutdown_listener: Listener,
    ) {
        loop {
            tokio::select! {
                _ = shutdown_listener.wait() => return ,
                _ = time::sleep(DEFAULT_LEASE_REQUEST_TIME) => {}
            }
            // only leader will check expired lease
            if lease_server.lease_storage.is_primary() {
                for id in lease_server.lease_storage.find_expired_leases() {
                    let _handle = tokio::spawn({
                        let s = Arc::clone(&lease_server);
                        let token_option = lease_server.auth_storage.root_token();
                        async move {
                            let mut request =
                                xlinerpc::Request::from_data(LeaseRevokeRequest { id });
                            if let Ok(token) = token_option {
                                request.metadata_mut().insert("token", token);
                            }
                            if let Err(e) = s.lease_revoke(request).await {
                                warn!("Failed to revoke expired leases: {}", e);
                            }
                        }
                    });
                }
            }
        }
    }

    /// Propose request and get result with fast/slow path
    async fn propose<T>(
        &self,
        request: xlinerpc::Request<T>,
    ) -> Result<(CommandResponse, Option<SyncResponse>), Status>
    where
        T: Into<RequestWrapper>,
    {
        let auth_info = self
            .auth_storage
            .try_get_auth_info_from_rpc_request(&request)?;
        let request = request.into_inner().into();
        let cmd = Command::new_with_auth_info(request, auth_info);
        let res = self.client.propose(&cmd, None, false).await??;
        Ok(res)
    }

    /// Handle keep alive at leader
    #[allow(
        dead_code,
        clippy::arithmetic_side_effects,
        clippy::ignored_unit_patterns,
        clippy::result_large_err
    )] // Introduced by tokio::select!
    fn leader_keep_alive(
        &self,
        request_stream: Streaming<LeaseKeepAliveRequest>,
    ) -> Result<KeepAliveStream, Status> {
        self.leader_keep_alive_stream(request_stream)
    }

    /// Handle keep alive at leader from an arbitrary request stream source.
    #[allow(
        clippy::arithmetic_side_effects,
        clippy::ignored_unit_patterns,
        clippy::result_large_err
    )]
    fn leader_keep_alive_stream<ST>(
        &self,
        mut request_stream: ST,
    ) -> Result<KeepAliveStream, Status>
    where
        ST: Stream<Item = Result<LeaseKeepAliveRequest, Status>> + Unpin + Send + 'static,
    {
        let shutdown_listener = self
            .task_manager
            .get_shutdown_listener(TaskName::LeaseKeepAlive)
            .ok_or(Status::cancelled("The cluster is shutting down"))?;
        let lease_storage = Arc::clone(&self.lease_storage);
        let stream = try_stream! {
           loop {
                let keep_alive_req: LeaseKeepAliveRequest = tokio::select! {
                    _ = shutdown_listener.wait() => {
                        debug!("Lease keep alive shutdown");
                        break;
                    }
                    res = request_stream.next() => {
                        if let Some(Ok(keep_alive_req)) = res {
                            keep_alive_req
                        } else {
                            break;
                        }
                    }
                };
                debug!("Receive LeaseKeepAliveRequest {:?}", keep_alive_req);
                let ttl = if lease_storage.is_primary() {
                    tokio::select! {
                        _ = shutdown_listener.wait() => {
                            debug!("Lease keep alive shutdown");
                            break;
                        }
                        _ = lease_storage.wait_synced(keep_alive_req.id) => {
                        }
                    };
                    lease_storage.keep_alive(keep_alive_req.id).map_err(Into::into)
                } else {
                    Err(Status::failed_precondition("current node is not a leader"))
                }?;
                yield LeaseKeepAliveResponse {
                    id: keep_alive_req.id,
                    ttl,
                    ..LeaseKeepAliveResponse::default()
                };
            }
        };
        Ok(Box::pin(stream))
    }

    /// Handle keep alive at follower
    #[allow(
        dead_code,
        clippy::arithmetic_side_effects,
        clippy::ignored_unit_patterns
    )] // Introduced by tokio::select!
    async fn follower_keep_alive(
        &self,
        request_stream: Streaming<LeaseKeepAliveRequest>,
        leader_addrs: &[String],
    ) -> Result<KeepAliveStream, Status> {
        self.follower_keep_alive_stream(request_stream, leader_addrs)
            .await
    }

    /// Handle keep alive at follower from an arbitrary request stream source.
    #[allow(clippy::arithmetic_side_effects, clippy::ignored_unit_patterns)]
    async fn follower_keep_alive_stream<ST>(
        &self,
        mut request_stream: ST,
        leader_addrs: &[String],
    ) -> Result<KeepAliveStream, Status>
    where
        ST: Stream<Item = Result<LeaseKeepAliveRequest, Status>> + Send + Unpin + 'static,
    {
        let shutdown_listener = self
            .task_manager
            .get_shutdown_listener(TaskName::LeaseKeepAlive)
            .ok_or(Status::cancelled("The cluster is shutting down"))?;
        let channel = QuicChannel::with_addrs(
            Arc::clone(&self.quic_client),
            leader_addrs.to_vec(),
            DnsFallback::Disabled,
        );

        let redirect_stream = stream! {
            loop {
                tokio::select! {
                    _ = shutdown_listener.wait() => {
                        debug!("Lease keep alive shutdown");
                        break;
                    }
                    res = request_stream.next() => {
                        if let Some(Ok(keep_alive_req)) = res {
                            yield keep_alive_req;
                        } else {
                            break;
                        }
                    }
                }
            }

        };

        let stream = channel
            .bidirectional_streaming_call::<LeaseKeepAliveRequest, LeaseKeepAliveResponse>(
                MethodId::XlineLeaseKeepAlive,
                Box::pin(redirect_stream),
                Vec::new(),
                NO_FORWARD_TIMEOUT,
            )
            .await?
            .map(|r| r.map_err(Status::from));

        Ok(Box::pin(stream))
    }

    /// Start a lease keep-alive stream from an arbitrary request stream source.
    pub(crate) async fn lease_keep_alive_stream<ST>(
        &self,
        request_stream: ST,
    ) -> Result<KeepAliveStream, Status>
    where
        ST: Stream<Item = Result<LeaseKeepAliveRequest, Status>> + Send + Unpin + 'static,
    {
        loop {
            if self.lease_storage.is_primary() {
                break Ok(self.leader_keep_alive_stream(request_stream)?);
            }
            let leader_id = self.client.fetch_leader_id(false).await?;
            if !self.lease_storage.is_primary() {
                let leader_addrs = self.cluster_info.client_urls(leader_id).unwrap_or_else(|| {
                    unreachable!(
                        "The address of leader {} not found in all_members {:?}",
                        leader_id, self.cluster_info
                    )
                });
                break Ok(self
                    .follower_keep_alive_stream(request_stream, &leader_addrs)
                    .await?);
            }
        }
    }

    /// `LeaseGrant` creates a lease which expires if the server does not receive a `keepAlive`
    /// within a given time to live period. All keys attached to the lease will be expired and
    /// deleted if the lease expires. Each expired key generates a delete event in the event history.
    async fn lease_grant(
        &self,
        mut request: xlinerpc::Request<LeaseGrantRequest>,
    ) -> Result<xlinerpc::Response<LeaseGrantResponse>, Status> {
        debug!("Receive LeaseGrantRequest {:?}", request);
        let lease_grant_req = request.get_mut();
        if lease_grant_req.id == 0 {
            lease_grant_req.id = self.id_gen.next();
        }

        let (res, sync_res) = self.propose(request).await?;

        let mut res: LeaseGrantResponse = res.into_inner().into();
        if let Some(sync_res) = sync_res {
            let revision = sync_res.revision();
            debug!("Get revision {:?} for LeaseGrantResponse", revision);
            if let Some(header) = res.header.as_mut() {
                header.revision = revision;
            }
        }
        Ok(xlinerpc::Response::from_data(res))
    }

    /// `LeaseRevoke` revokes a lease. All keys attached to the lease will expire and be deleted.
    pub(crate) async fn lease_revoke(
        &self,
        request: xlinerpc::Request<LeaseRevokeRequest>,
    ) -> Result<xlinerpc::Response<LeaseRevokeResponse>, Status> {
        debug!("Receive LeaseRevokeRequest {:?}", request);

        let (res, sync_res) = self.propose(request).await?;

        let mut res: LeaseRevokeResponse = res.into_inner().into();
        if let Some(sync_res) = sync_res {
            let revision = sync_res.revision();
            debug!("Get revision {:?} for LeaseRevokeResponse", revision);
            if let Some(header) = res.header.as_mut() {
                header.revision = revision;
            }
            metrics::get().lease_expired_total.add(1, &[]);
        }
        Ok(xlinerpc::Response::from_data(res))
    }

    /// `LeaseKeepAlive` keeps the lease alive by streaming keep alive requests from the client
    /// to the server and streaming keep alive responses from the server to the client.
    async fn lease_keep_alive(
        &self,
        request: xlinerpc::Request<Streaming<LeaseKeepAliveRequest>>,
    ) -> Result<
        xlinerpc::Response<
            Pin<Box<dyn Stream<Item = Result<LeaseKeepAliveResponse, Status>> + Send>>,
        >,
        Status,
    > {
        debug!("Receive LeaseKeepAliveRequest {:?}", request);
        let stream = self.lease_keep_alive_stream(request.into_inner()).await?;
        Ok(xlinerpc::Response::from_data(stream))
    }

    /// `LeaseTimeToLive` retrieves lease information.
    pub(crate) async fn lease_time_to_live(
        &self,
        request: xlinerpc::Request<LeaseTimeToLiveRequest>,
    ) -> Result<xlinerpc::Response<LeaseTimeToLiveResponse>, Status> {
        debug!("Receive LeaseTimeToLiveRequest {:?}", request);
        let req = request.into_inner();
        loop {
            if self.lease_storage.is_primary() {
                let time_to_live_req = req.clone();

                self.lease_storage.wait_synced(time_to_live_req.id).await;

                let Some(lease) = self.lease_storage.look_up(time_to_live_req.id) else {
                    return Err(ExecuteError::LeaseNotFound(time_to_live_req.id).into());
                };

                let keys = if time_to_live_req.keys {
                    lease.keys()
                } else {
                    Vec::new()
                };
                let res = LeaseTimeToLiveResponse {
                    header: Some(self.lease_storage.gen_header()),
                    id: time_to_live_req.id,
                    ttl: lease.remaining().as_secs().numeric_cast(),
                    granted_ttl: lease.ttl().as_secs().numeric_cast(),
                    keys,
                };
                return Ok(xlinerpc::Response::from_data(res));
            }
            let leader_id = self.client.fetch_leader_id(false).await?;
            let leader_addrs = self.cluster_info.client_urls(leader_id).unwrap_or_else(|| {
                unreachable!(
                    "The address of leader {} not found in all_members {:?}",
                    leader_id, self.cluster_info
                )
            });
            if !self.lease_storage.is_primary() {
                let channel = QuicChannel::with_addrs(
                    Arc::clone(&self.quic_client),
                    leader_addrs.to_vec(),
                    DnsFallback::Disabled,
                );
                let res = channel
                    .unary_call::<LeaseTimeToLiveRequest, LeaseTimeToLiveResponse>(
                        MethodId::XlineLeaseTtl,
                        req.clone(),
                        Vec::new(),
                        NO_FORWARD_TIMEOUT,
                    )
                    .await?;
                return Ok(xlinerpc::Response::from_data(res));
            }
        }
    }

    /// `LeaseLeases` lists all existing leases.
    async fn lease_leases(
        &self,
        request: xlinerpc::Request<LeaseLeasesRequest>,
    ) -> Result<xlinerpc::Response<LeaseLeasesResponse>, Status> {
        debug!("Receive LeaseLeasesRequest {:?}", request);

        let (res, sync_res) = self.propose(request).await?;

        let mut res: LeaseLeasesResponse = res.into_inner().into();
        if let Some(sync_res) = sync_res {
            let revision = sync_res.revision();
            debug!("Get revision {:?} for LeaseLeasesResponse", revision);
            if let Some(header) = res.header.as_mut() {
                header.revision = revision;
            }
        }
        Ok(xlinerpc::Response::from_data(res))
    }
}

pub(crate) struct Server {
    lease_server: Arc<LeaseServer>,
}
impl Server {
    #[allow(unused)]
    pub(crate) fn new(lease_server: LeaseServer) -> Self {
        Self {
            lease_server: Arc::new(lease_server),
        }
    }
    pub(crate) fn from_arc(lease_server: Arc<LeaseServer>) -> Self {
        Self { lease_server }
    }
    pub(crate) fn endpoint(self) -> RouterEndpoint<Arc<LeaseServer>> {
        RouterEndpoint::new(self.lease_server)
            .add_unary_fn(
                "/LeaseGrant",
                move |this: Arc<LeaseServer>, request: xlinerpc::Request<LeaseGrantRequest>| async move {
                    this.lease_grant(request).await
                },
            )
            .add_unary_fn(
                "/LeaseRevoke",
                move |this: Arc<LeaseServer>, request: xlinerpc::Request<LeaseRevokeRequest>| async move {
                    this.lease_revoke(request).await
                },
            )
            .add_streaming_fn(
                "/LeaseKeepAlive",
                move |this: Arc<LeaseServer>, request: xlinerpc::Request<Streaming<LeaseKeepAliveRequest>>| async move {
                    this.lease_keep_alive(request).await
                },
            )
            .add_unary_fn(
                "/LeaseTimeToLive",
                move |this: Arc<LeaseServer>, request: xlinerpc::Request<LeaseTimeToLiveRequest>| async move {
                    this.lease_time_to_live(request).await
                },
            )
            .add_unary_fn(
                "/LeaseLeases",
                move |this: Arc<LeaseServer>, request: xlinerpc::Request<LeaseLeasesRequest>| async move {
                    this.lease_leases(request).await
                },
            )
    }
}
