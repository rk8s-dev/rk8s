use std::{pin::Pin, sync::Arc, time::Duration};

use async_stream::{stream, try_stream};
use clippy_utilities::NumericCast;
use curp::members::ClusterInfo;
use futures::stream::Stream;
use tokio::time;
use xlinerpc::status::Status;
use tonic::transport::{ClientTlsConfig, Endpoint};
use tracing::{debug, warn};
use utils::{
    build_endpoint,
    task_manager::{Listener, TaskManager, tasks::TaskName},
};
use xlineapi::{
    command::{Command, CommandResponse, CurpClient, SyncResponse},
    execute_error::ExecuteError,
};

use crate::{
    id_gen::IdGenerator,
    metrics,
    rpc::{
        Lease, LeaseClient, LeaseGrantRequest, LeaseGrantResponse, LeaseKeepAliveRequest,
        LeaseKeepAliveResponse, LeaseLeasesRequest, LeaseLeasesResponse, LeaseRevokeRequest,
        LeaseRevokeResponse, LeaseTimeToLiveRequest, LeaseTimeToLiveResponse, RequestWrapper,
    },
    storage::{AuthStore, LeaseStore},
};

/// Default Lease Request Time
const DEFAULT_LEASE_REQUEST_TIME: Duration = Duration::from_millis(500);

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
    /// Client tls config
    client_tls_config: Option<ClientTlsConfig>,
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
        client_tls_config: Option<ClientTlsConfig>,
        task_manager: &Arc<TaskManager>,
    ) -> Arc<Self> {
        let lease_server = Arc::new(Self {
            lease_storage,
            auth_storage,
            client,
            id_gen,
            cluster_info,
            client_tls_config,
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
                            let mut request = xlinerpc::Request::from_data(LeaseRevokeRequest { id });
                            if let Ok(token) = token_option {
                                request.meta_mut().insert("token", token.as_bytes());
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
        let auth_info = self.auth_storage.try_get_auth_info_from_request(&request)?;
        let request = request.into_inner().into();
        let cmd = Command::new_with_auth_info(request, auth_info);
        let res = self.client.propose(&cmd, None, false).await??;
        Ok(res)
    }

    /// Handle keep alive at leader
    #[allow(
        clippy::arithmetic_side_effects,
        clippy::ignored_unit_patterns,
        clippy::result_large_err
    )] // Introduced by tokio::select!
    fn leader_keep_alive(
        &self,
        mut request_stream: impl Stream<Item = Result<LeaseKeepAliveRequest, Status>> + Send,
    ) -> Result<KeepAliveStream, Status> {
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
                    res = request_stream.message() => {
                        if let Ok(Some(keep_alive_req)) = res {
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
    #[allow(clippy::arithmetic_side_effects, clippy::ignored_unit_patterns)] // Introduced by tokio::select!
    async fn follower_keep_alive(
        &self,
        mut request_stream: impl Stream<Item = Result<LeaseKeepAliveRequest, Status>> + Send,
        leader_addrs: &[String],
    ) -> Result<KeepAliveStream, Status> {
        let shutdown_listener = self
            .task_manager
            .get_shutdown_listener(TaskName::LeaseKeepAlive)
            .ok_or(Status::cancelled("The cluster is shutting down"))?;
        let endpoints = build_endpoints(leader_addrs, self.client_tls_config.as_ref())?;
        let channel = tonic::transport::Channel::balance_list(endpoints.into_iter());
        let mut lease_client = LeaseClient::new(channel);

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

        let stream = lease_client
            .lease_keep_alive(redirect_stream)
            .await?
            .into_inner();

        Ok(Box::pin(stream))
    }
}

/// Build endpoints from addresses
#[allow(clippy::result_large_err)]
fn build_endpoints(
    addrs: &[String],
    tls_config: Option<&ClientTlsConfig>,
) -> Result<Vec<Endpoint>, Status> {
    addrs
        .iter()
        .map(|addr| {
            let endpoint =
                build_endpoint(addr, tls_config).map_err(|e| Status::internal(e.to_string()))?;
            Ok(endpoint)
        })
        .collect()
}


impl Lease for LeaseServer {
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
    async fn lease_revoke(
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

    /// Server streaming response type for the `LeaseKeepAlive` method.
    type LeaseKeepAliveStream =
        Pin<Box<dyn Stream<Item = Result<LeaseKeepAliveResponse, Status>> + Send>>;

    /// `LeaseKeepAlive` keeps the lease alive by streaming keep alive requests from the client
    /// to the server and streaming keep alive responses from the server to the client.
    async fn lease_keep_alive(
        &self,
        request: xlinerpc::Request<impl Stream<Item = Result<LeaseKeepAliveRequest, Status>> + Send>,
    ) -> Result<xlinerpc::Response<Self::LeaseKeepAliveStream>, Status> {
        debug!("Receive LeaseKeepAliveRequest {:?}", request);
        let request_stream = request.into_inner();
        let stream = loop {
            if self.lease_storage.is_primary() {
                break self.leader_keep_alive(request_stream)?;
            }
            let leader_id = self.client.fetch_leader_id(false).await?;
            // Given that a candidate server may become a leader when it won the election or
            // a follower when it lost the election. Therefore we need to double check here.
            // We can directly invoke leader_keep_alive when a candidate becomes a leader.
            if !self.lease_storage.is_primary() {
                let leader_addrs = self.cluster_info.client_urls(leader_id).unwrap_or_else(|| {
                    unreachable!(
                        "The address of leader {} not found in all_members {:?}",
                        leader_id, self.cluster_info
                    )
                });
                break self
                    .follower_keep_alive(request_stream, &leader_addrs)
                    .await?;
            }
        };
        Ok(xlinerpc::Response::from_data(stream))
    }

    /// `LeaseTimeToLive` retrieves lease information.
    async fn lease_time_to_live(
        &self,
        request: xlinerpc::Request<LeaseTimeToLiveRequest>,
    ) -> Result<xlinerpc::Response<LeaseTimeToLiveResponse>, Status> {
        debug!("Receive LeaseTimeToLiveRequest {:?}", request);
        loop {
            if self.lease_storage.is_primary() {
                let time_to_live_req = request.into_inner();

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
                let endpoints = build_endpoints(&leader_addrs, self.client_tls_config.as_ref())?;
                let channel = tonic::transport::Channel::balance_list(endpoints.into_iter());
                let mut lease_client = LeaseClient::new(channel);
                return lease_client.lease_time_to_live(request).await;
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