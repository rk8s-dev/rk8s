//! Asynchronous client & synchronous client.

#[cfg(feature = "raw-channel")]
use crate::channel::Channel;
use crate::error::{Error, Result};
use crate::intercept::{InterceptedChannel, Interceptor};
use crate::lock::RwLockExt;
#[cfg(feature = "tls-openssl")]
use crate::openssl_tls::{OpenSslClientConfig, OpenSslConnector};
use crate::rpc::auth::Permission;
use crate::rpc::auth::{AuthClient, AuthDisableResponse, AuthEnableResponse};
use crate::rpc::auth::{
    RoleAddResponse, RoleDeleteResponse, RoleGetResponse, RoleGrantPermissionResponse,
    RoleListResponse, RoleRevokePermissionOptions, RoleRevokePermissionResponse, UserAddOptions,
    UserAddResponse, UserChangePasswordResponse, UserDeleteResponse, UserGetResponse,
    UserGrantRoleResponse, UserListResponse, UserRevokeRoleResponse,
};
use crate::rpc::cluster::{
    ClusterClient, MemberAddOptions, MemberAddResponse, MemberListResponse, MemberPromoteResponse,
    MemberRemoveResponse, MemberUpdateResponse,
};
use crate::rpc::election::{
    CampaignResponse, ElectionClient, LeaderResponse, ObserveStream, ProclaimOptions,
    ProclaimResponse, ResignOptions, ResignResponse,
};
use crate::rpc::kv::{
    CompactionOptions, CompactionResponse, DeleteOptions, DeleteResponse, GetOptions, GetResponse,
    KvClient, PutOptions, PutResponse, Txn, TxnResponse,
};
use crate::rpc::lease::{
    LeaseClient, LeaseGrantOptions, LeaseGrantResponse, LeaseKeepAliveStream, LeaseKeeper,
    LeaseLeasesResponse, LeaseRevokeResponse, LeaseTimeToLiveOptions, LeaseTimeToLiveResponse,
};
use crate::rpc::lock::{LockClient, LockOptions, LockResponse, UnlockResponse};
use crate::rpc::maintenance::{
    AlarmAction, AlarmOptions, AlarmResponse, AlarmType, DefragmentResponse, HashKvResponse,
    HashResponse, MaintenanceClient, MoveLeaderResponse, SnapshotStreaming, StatusResponse,
};
use crate::rpc::watch::{WatchClient, WatchOptions, WatchStream, Watcher};
#[cfg(feature = "tls-openssl")]
use crate::OpenSslResult;
#[cfg(feature = "tls")]
use crate::TlsOptions;
use http::uri::Uri;
use http::HeaderValue;

use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc::Sender;

use tonic::transport::{channel::Change, Endpoint};

const HTTP_PREFIX: &str = "http://";
const HTTPS_PREFIX: &str = "https://";

/// Asynchronous `etcd` client using v3 API.
#[derive(Clone)]
pub struct Client {
    kv: KvClient,
    watch: WatchClient,
    lease: LeaseClient,
    lock: LockClient,
    auth: AuthClient,
    maintenance: MaintenanceClient,
    cluster: ClusterClient,
    election: ElectionClient,
    options: Option<ConnectOptions>,
    tx: Option<Sender<Change<Uri, Endpoint>>>,
}

impl Client {
    /// Connect to `etcd` servers from given `endpoints`.
    pub async fn connect<E: AsRef<str>, S: AsRef<[E]>>(
        endpoints: S,
        options: Option<ConnectOptions>,
    ) -> Result<Self> {
        #[cfg(not(feature = "tls-openssl"))]
        let make_balanced_channel = crate::channel::Tonic;
        #[cfg(feature = "tls-openssl")]
        let make_balanced_channel = crate::channel::Openssl {
            conn: options
                .clone()
                .and_then(|o| o.otls)
                .unwrap_or_else(OpenSslConnector::create_default)?,
        };
        Self::connect_with_balanced_channel(endpoints, options, make_balanced_channel).await
    }

    /// Connect to `etcd` servers from given `endpoints` and a balanced channel.
    pub async fn connect_with_balanced_channel<E: AsRef<str>, S: AsRef<[E]>, MBC>(
        endpoints: S,
        options: Option<ConnectOptions>,
        make_balanced_channel: MBC,
    ) -> Result<Self>
    where
        MBC: crate::channel::BalancedChannelBuilder,
        crate::error::Error: From<MBC::Error>,
    {
        let endpoints = {
            let mut eps = Vec::new();
            for e in endpoints.as_ref() {
                let channel = Self::build_endpoint(e.as_ref(), &options)?;
                eps.push(channel);
            }
            eps
        };

        if endpoints.is_empty() {
            return Err(Error::InvalidArgs(String::from("empty endpoints")));
        }

        // Always use balance strategy even if there is only one endpoint.
        let (channel, tx) = make_balanced_channel.balanced_channel(64)?;
        let channel = InterceptedChannel::new(
            channel,
            Interceptor {
                require_leader: options.as_ref().map(|o| o.require_leader).unwrap_or(false),
            },
        );
        for endpoint in endpoints {
            // The rx inside `channel` won't be closed or dropped here
            tx.send(Change::Insert(endpoint.uri().clone(), endpoint))
                .await
                .unwrap();
        }

        let mut options = options;

        let auth_token = Arc::new(RwLock::new(None));
        Self::auth(channel.clone(), &mut options, &auth_token).await?;

        Ok(Self::build_client(channel, Some(tx), auth_token, options))
    }

    #[cfg(feature = "raw-channel")]
    /// Connect to `etcd` servers represented by the given `channel`.
    pub async fn from_channel(channel: Channel, options: Option<ConnectOptions>) -> Result<Self> {
        let channel = InterceptedChannel::new(
            channel,
            Interceptor {
                require_leader: options.as_ref().map(|o| o.require_leader).unwrap_or(false),
            },
        );
        let mut options = options;

        let auth_token = Arc::new(RwLock::new(None));
        Self::auth(channel.clone(), &mut options, &auth_token).await?;

        Ok(Self::build_client(channel, None, auth_token, options))
    }

    fn build_endpoint(url: &str, options: &Option<ConnectOptions>) -> Result<Endpoint> {
        use tonic::transport::Channel as TonicChannel;
        let mut endpoint = if url.starts_with(HTTP_PREFIX) {
            #[cfg(feature = "tls")]
            if let Some(connect_options) = options {
                if connect_options.tls.is_some() {
                    return Err(Error::InvalidArgs(String::from(
                        "TLS options are only supported with HTTPS URLs",
                    )));
                }
            }

            TonicChannel::builder(url.parse()?)
        } else if url.starts_with(HTTPS_PREFIX) {
            #[cfg(not(any(feature = "tls", feature = "tls-openssl")))]
            return Err(Error::InvalidArgs(String::from(
                "HTTPS URLs are only supported with the feature \"tls\"",
            )));

            #[cfg(all(feature = "tls-openssl", not(feature = "tls")))]
            {
                TonicChannel::builder(url.parse()?)
            }

            #[cfg(feature = "tls")]
            {
                let tls = if let Some(connect_options) = options {
                    connect_options.tls.clone()
                } else {
                    None
                }
                .unwrap_or_else(TlsOptions::new);

                TonicChannel::builder(url.parse()?).tls_config(tls)?
            }
        } else {
            #[cfg(feature = "tls")]
            {
                let tls = if let Some(connect_options) = options {
                    connect_options.tls.clone()
                } else {
                    None
                };

                match tls {
                    Some(tls) => {
                        let e = HTTPS_PREFIX.to_owned() + url;
                        TonicChannel::builder(e.parse()?).tls_config(tls)?
                    }
                    None => {
                        let e = HTTP_PREFIX.to_owned() + url;
                        TonicChannel::builder(e.parse()?)
                    }
                }
            }

            #[cfg(all(feature = "tls-openssl", not(feature = "tls")))]
            {
                let pfx = if options.as_ref().and_then(|o| o.otls.as_ref()).is_some() {
                    HTTPS_PREFIX
                } else {
                    HTTP_PREFIX
                };
                let e = pfx.to_owned() + url;
                TonicChannel::builder(e.parse()?)
            }

            #[cfg(all(not(feature = "tls"), not(feature = "tls-openssl")))]
            {
                let e = HTTP_PREFIX.to_owned() + url;
                TonicChannel::builder(e.parse()?)
            }
        };

        if let Some(opts) = options {
            if let Some((interval, timeout)) = opts.keep_alive {
                endpoint = endpoint
                    .keep_alive_while_idle(opts.keep_alive_while_idle)
                    .http2_keep_alive_interval(interval)
                    .keep_alive_timeout(timeout);
            }

            if let Some(timeout) = opts.timeout {
                endpoint = endpoint.timeout(timeout);
            }

            if let Some(timeout) = opts.connect_timeout {
                endpoint = endpoint.connect_timeout(timeout);
            }

            if let Some(tcp_keepalive) = opts.tcp_keepalive {
                endpoint = endpoint.tcp_keepalive(Some(tcp_keepalive));
            }
        }

        Ok(endpoint)
    }

    async fn auth(
        channel: InterceptedChannel,
        options: &mut Option<ConnectOptions>,
        auth_token: &Arc<RwLock<Option<HeaderValue>>>,
    ) -> Result<()> {
        let user = match options {
            None => return Ok(()),
            Some(opt) => {
                // Take away the user, the password should not be stored in client.
                opt.user.take()
            }
        };

        if let Some((name, password)) = user {
            let mut tmp_auth = AuthClient::new(channel, auth_token.clone());
            let resp = tmp_auth.authenticate(name, password).await?;
            auth_token.write_unpoisoned().replace(resp.token().parse()?);
        }

        Ok(())
    }

    fn build_client(
        channel: InterceptedChannel,
        tx: Option<Sender<Change<Uri, Endpoint>>>,
        auth_token: Arc<RwLock<Option<HeaderValue>>>,
        options: Option<ConnectOptions>,
    ) -> Self {
        let kv = KvClient::new(channel.clone(), auth_token.clone());
        let watch = WatchClient::new(channel.clone(), auth_token.clone());
        let lease = LeaseClient::new(channel.clone(), auth_token.clone());
        let lock = LockClient::new(channel.clone(), auth_token.clone());
        let auth = AuthClient::new(channel.clone(), auth_token.clone());
        let cluster = ClusterClient::new(channel.clone(), auth_token.clone());
        let maintenance = MaintenanceClient::new(channel.clone(), auth_token.clone());
        let election = ElectionClient::new(channel, auth_token);

        Self {
            kv,
            watch,
            lease,
            lock,
            auth,
            maintenance,
            cluster,
            election,
            options,
            tx,
        }
    }

    /// Dynamically add an endpoint to the client.
    ///
    /// Which can be used to add a new member to the underlying balance cache.
    /// The typical scenario is that application can use a services discovery
    /// to discover the member list changes and add/remove them to/from the client.
    ///
    /// Note that the [`Client`] doesn't check the authentication before added.
    /// So the etcd member of the added endpoint REQUIRES to use the same auth
    /// token as when create the client. Otherwise, the underlying balance
    /// services will not be able to connect to the new endpoint.
    #[inline]
    pub async fn add_endpoint<E: AsRef<str>>(&self, endpoint: E) -> Result<()> {
        let endpoint = Self::build_endpoint(endpoint.as_ref(), &self.options)?;
        let Some(tx) = &self.tx else {
            return Err(Error::EndpointsNotManaged);
        };
        tx.send(Change::Insert(endpoint.uri().clone(), endpoint))
            .await
            .map_err(|e| Error::EndpointError(format!("failed to add endpoint because of {e}")))
    }

    /// Dynamically remove an endpoint from the client.
    ///
    /// Note that the `endpoint` str should be the same as it was added.
    /// And the underlying balance services cache used the hash from the Uri,
    /// which was parsed from `endpoint` str, to do the equality comparisons.
    #[inline]
    pub async fn remove_endpoint<E: AsRef<str>>(&self, endpoint: E) -> Result<()> {
        let uri = http::Uri::from_str(endpoint.as_ref())?;
        let Some(tx) = &self.tx else {
            return Err(Error::EndpointsNotManaged);
        };
        tx.send(Change::Remove(uri))
            .await
            .map_err(|e| Error::EndpointError(format!("failed to remove endpoint because of {e}")))
    }

    /// Gets a KV client.
    #[inline]
    pub fn kv_client(&self) -> KvClient {
        self.kv.clone()
    }

    /// Gets a watch client.
    #[inline]
    pub fn watch_client(&self) -> WatchClient {
        self.watch.clone()
    }

    /// Gets a lease client.
    #[inline]
    pub fn lease_client(&self) -> LeaseClient {
        self.lease.clone()
    }

    /// Gets an auth client.
    #[inline]
    pub fn auth_client(&self) -> AuthClient {
        self.auth.clone()
    }

    /// Gets a maintenance client.
    #[inline]
    pub fn maintenance_client(&self) -> MaintenanceClient {
        self.maintenance.clone()
    }

    /// Gets a cluster client.
    #[inline]
    pub fn cluster_client(&self) -> ClusterClient {
        self.cluster.clone()
    }

    /// Gets a lock client.
    #[inline]
    pub fn lock_client(&self) -> LockClient {
        self.lock.clone()
    }

    /// Gets a election client.
    #[inline]
    pub fn election_client(&self) -> ElectionClient {
        self.election.clone()
    }

    /// Put the given key into the key-value store.
    /// A put request increments the revision of the key-value store
    /// and generates one event in the event history.
    #[inline]
    pub async fn put(
        &mut self,
        key: impl Into<Vec<u8>>,
        value: impl Into<Vec<u8>>,
        options: Option<PutOptions>,
    ) -> Result<PutResponse> {
        self.kv.put(key, value, options).await
    }

    /// Gets the key from the key-value store.
    #[inline]
    pub async fn get(
        &mut self,
        key: impl Into<Vec<u8>>,
        options: Option<GetOptions>,
    ) -> Result<GetResponse> {
        self.kv.get(key, options).await
    }

    /// Deletes the given key from the key-value store.
    #[inline]
    pub async fn delete(
        &mut self,
        key: impl Into<Vec<u8>>,
        options: Option<DeleteOptions>,
    ) -> Result<DeleteResponse> {
        self.kv.delete(key, options).await
    }

    /// Compacts the event history in the etcd key-value store. The key-value
    /// store should be periodically compacted or the event history will continue to grow
    /// indefinitely.
    #[inline]
    pub async fn compact(
        &mut self,
        revision: i64,
        options: Option<CompactionOptions>,
    ) -> Result<CompactionResponse> {
        self.kv.compact(revision, options).await
    }

    /// Processes multiple operations in a single transaction.
    /// A txn request increments the revision of the key-value store
    /// and generates events with the same revision for every completed operation.
    /// It is not allowed to modify the same key several times within one txn.
    #[inline]
    pub async fn txn(&mut self, txn: Txn) -> Result<TxnResponse> {
        self.kv.txn(txn).await
    }

    /// Watches for events happening or that have happened. Both input and output
    /// are streams; the input stream is for creating and canceling watcher and the output
    /// stream sends events. The entire event history can be watched starting from the
    /// last compaction revision.
    #[inline]
    pub async fn watch(
        &mut self,
        key: impl Into<Vec<u8>>,
        options: Option<WatchOptions>,
    ) -> Result<(Watcher, WatchStream)> {
        self.watch.watch(key, options).await
    }

    /// Creates a lease which expires if the server does not receive a keepAlive
    /// within a given time to live period. All keys attached to the lease will be expired and
    /// deleted if the lease expires. Each expired key generates a delete event in the event history.
    #[inline]
    pub async fn lease_grant(
        &mut self,
        ttl: i64,
        options: Option<LeaseGrantOptions>,
    ) -> Result<LeaseGrantResponse> {
        self.lease.grant(ttl, options).await
    }

    /// Revokes a lease. All keys attached to the lease will expire and be deleted.
    #[inline]
    pub async fn lease_revoke(&mut self, id: i64) -> Result<LeaseRevokeResponse> {
        self.lease.revoke(id).await
    }

    /// Keeps the lease alive by streaming keep alive requests from the client
    /// to the server and streaming keep alive responses from the server to the client.
    #[inline]
    pub async fn lease_keep_alive(
        &mut self,
        id: i64,
    ) -> Result<(LeaseKeeper, LeaseKeepAliveStream)> {
        self.lease.keep_alive(id).await
    }

    /// Retrieves lease information.
    #[inline]
    pub async fn lease_time_to_live(
        &mut self,
        id: i64,
        options: Option<LeaseTimeToLiveOptions>,
    ) -> Result<LeaseTimeToLiveResponse> {
        self.lease.time_to_live(id, options).await
    }

    /// Lists all existing leases.
    #[inline]
    pub async fn leases(&mut self) -> Result<LeaseLeasesResponse> {
        self.lease.leases().await
    }

    /// Lock acquires a distributed shared lock on a given named lock.
    /// On success, it will return a unique key that exists so long as the
    /// lock is held by the caller. This key can be used in conjunction with
    /// transactions to safely ensure updates to etcd only occur while holding
    /// lock ownership. The lock is held until Unlock is called on the key or the
    /// lease associate with the owner expires.
    #[inline]
    pub async fn lock(
        &mut self,
        name: impl Into<Vec<u8>>,
        options: Option<LockOptions>,
    ) -> Result<LockResponse> {
        self.lock.lock(name, options).await
    }

    /// Unlock takes a key returned by Lock and releases the hold on lock. The
    /// next Lock caller waiting for the lock will then be woken up and given
    /// ownership of the lock.
    #[inline]
    pub async fn unlock(&mut self, key: impl Into<Vec<u8>>) -> Result<UnlockResponse> {
        self.lock.unlock(key).await
    }

    /// Enables authentication.
    #[inline]
    pub async fn auth_enable(&mut self) -> Result<AuthEnableResponse> {
        self.auth.auth_enable().await
    }

    /// Disables authentication.
    #[inline]
    pub async fn auth_disable(&mut self) -> Result<AuthDisableResponse> {
        self.auth.auth_disable().await
    }

    /// Adds role.
    #[inline]
    pub async fn role_add(&mut self, name: impl Into<String>) -> Result<RoleAddResponse> {
        self.auth.role_add(name).await
    }

    /// Deletes role.
    #[inline]
    pub async fn role_delete(&mut self, name: impl Into<String>) -> Result<RoleDeleteResponse> {
        self.auth.role_delete(name).await
    }

    /// Gets role.
    #[inline]
    pub async fn role_get(&mut self, name: impl Into<String>) -> Result<RoleGetResponse> {
        self.auth.role_get(name).await
    }

    /// Lists role.
    #[inline]
    pub async fn role_list(&mut self) -> Result<RoleListResponse> {
        self.auth.role_list().await
    }

    /// Grants role permission.
    #[inline]
    pub async fn role_grant_permission(
        &mut self,
        name: impl Into<String>,
        perm: Permission,
    ) -> Result<RoleGrantPermissionResponse> {
        self.auth.role_grant_permission(name, perm).await
    }

    /// Revokes role permission.
    #[inline]
    pub async fn role_revoke_permission(
        &mut self,
        name: impl Into<String>,
        key: impl Into<Vec<u8>>,
        options: Option<RoleRevokePermissionOptions>,
    ) -> Result<RoleRevokePermissionResponse> {
        self.auth.role_revoke_permission(name, key, options).await
    }

    /// Add an user.
    #[inline]
    pub async fn user_add(
        &mut self,
        name: impl Into<String>,
        password: impl Into<String>,
        options: Option<UserAddOptions>,
    ) -> Result<UserAddResponse> {
        self.auth.user_add(name, password, options).await
    }

    /// Gets the user info by the user name.
    #[inline]
    pub async fn user_get(&mut self, name: impl Into<String>) -> Result<UserGetResponse> {
        self.auth.user_get(name).await
    }

    /// Lists all users.
    #[inline]
    pub async fn user_list(&mut self) -> Result<UserListResponse> {
        self.auth.user_list().await
    }

    /// Deletes the given key from the key-value store.
    #[inline]
    pub async fn user_delete(&mut self, name: impl Into<String>) -> Result<UserDeleteResponse> {
        self.auth.user_delete(name).await
    }

    /// Change password for an user.
    #[inline]
    pub async fn user_change_password(
        &mut self,
        name: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<UserChangePasswordResponse> {
        self.auth.user_change_password(name, password).await
    }

    /// Grant role for an user.
    #[inline]
    pub async fn user_grant_role(
        &mut self,
        user: impl Into<String>,
        role: impl Into<String>,
    ) -> Result<UserGrantRoleResponse> {
        self.auth.user_grant_role(user, role).await
    }

    /// Revoke role for an user.
    #[inline]
    pub async fn user_revoke_role(
        &mut self,
        user: impl Into<String>,
        role: impl Into<String>,
    ) -> Result<UserRevokeRoleResponse> {
        self.auth.user_revoke_role(user, role).await
    }

    /// Maintain(get, active or inactive) alarms of members.
    #[inline]
    pub async fn alarm(
        &mut self,
        alarm_action: AlarmAction,
        alarm_type: AlarmType,
        options: Option<AlarmOptions>,
    ) -> Result<AlarmResponse> {
        self.maintenance
            .alarm(alarm_action, alarm_type, options)
            .await
    }

    /// Gets the status of a member.
    #[inline]
    pub async fn status(&mut self) -> Result<StatusResponse> {
        self.maintenance.status().await
    }

    /// Defragments a member's backend database to recover storage space.
    #[inline]
    pub async fn defragment(&mut self) -> Result<DefragmentResponse> {
        self.maintenance.defragment().await
    }

    /// Computes the hash of whole backend keyspace.
    /// including key, lease, and other buckets in storage.
    /// This is designed for testing ONLY!
    #[inline]
    pub async fn hash(&mut self) -> Result<HashResponse> {
        self.maintenance.hash().await
    }

    /// Computes the hash of all MVCC keys up to a given revision.
    /// It only iterates \"key\" bucket in backend storage.
    #[inline]
    pub async fn hash_kv(&mut self, revision: i64) -> Result<HashKvResponse> {
        self.maintenance.hash_kv(revision).await
    }

    /// Gets a snapshot of the entire backend from a member over a stream to a client.
    #[inline]
    pub async fn snapshot(&mut self) -> Result<SnapshotStreaming> {
        self.maintenance.snapshot().await
    }

    /// Adds current connected server as a member.
    #[inline]
    pub async fn member_add<E: AsRef<str>, S: AsRef<[E]>>(
        &mut self,
        urls: S,
        options: Option<MemberAddOptions>,
    ) -> Result<MemberAddResponse> {
        let mut eps = Vec::new();
        for e in urls.as_ref() {
            let e = e.as_ref();
            let url = if e.starts_with(HTTP_PREFIX) || e.starts_with(HTTPS_PREFIX) {
                e.to_string()
            } else {
                HTTP_PREFIX.to_owned() + e
            };
            eps.push(url);
        }

        self.cluster.member_add(eps, options).await
    }

    /// Remove a member.
    #[inline]
    pub async fn member_remove(&mut self, id: u64) -> Result<MemberRemoveResponse> {
        self.cluster.member_remove(id).await
    }

    /// Updates the member.
    #[inline]
    pub async fn member_update(
        &mut self,
        id: u64,
        url: impl Into<Vec<String>>,
    ) -> Result<MemberUpdateResponse> {
        self.cluster.member_update(id, url).await
    }

    /// Promotes the member.
    #[inline]
    pub async fn member_promote(&mut self, id: u64) -> Result<MemberPromoteResponse> {
        self.cluster.member_promote(id).await
    }

    /// Lists members.
    #[inline]
    pub async fn member_list(&mut self) -> Result<MemberListResponse> {
        self.cluster.member_list().await
    }

    /// Moves the current leader node to target node.
    #[inline]
    pub async fn move_leader(&mut self, target_id: u64) -> Result<MoveLeaderResponse> {
        self.maintenance.move_leader(target_id).await
    }

    /// Puts a value as eligible for the election on the prefix key.
    /// Multiple sessions can participate in the election for the
    /// same prefix, but only one can be the leader at a time.
    #[inline]
    pub async fn campaign(
        &mut self,
        name: impl Into<Vec<u8>>,
        value: impl Into<Vec<u8>>,
        lease: i64,
    ) -> Result<CampaignResponse> {
        self.election.campaign(name, value, lease).await
    }

    /// Lets the leader announce a new value without another election.
    #[inline]
    pub async fn proclaim(
        &mut self,
        value: impl Into<Vec<u8>>,
        options: Option<ProclaimOptions>,
    ) -> Result<ProclaimResponse> {
        self.election.proclaim(value, options).await
    }

    /// Returns the leader value for the current election.
    #[inline]
    pub async fn leader(&mut self, name: impl Into<Vec<u8>>) -> Result<LeaderResponse> {
        self.election.leader(name).await
    }

    /// Returns a channel that reliably observes ordered leader proposals
    /// as GetResponse values on every current elected leader key.
    #[inline]
    pub async fn observe(&mut self, name: impl Into<Vec<u8>>) -> Result<ObserveStream> {
        self.election.observe(name).await
    }

    /// Releases election leadership and then start a new election
    #[inline]
    pub async fn resign(&mut self, option: Option<ResignOptions>) -> Result<ResignResponse> {
        self.election.resign(option).await
    }

    /// Sets client-side authentication.
    pub async fn set_client_auth(&mut self, name: String, password: String) -> Result<()> {
        self.auth.set_client_auth(name, password).await
    }

    /// Removes client-side authentication.
    pub fn remove_client_auth(&mut self) {
        self.auth.remove_client_auth();
    }
}

/// Options for `Connect` operation.
#[derive(Debug, Default, Clone)]
pub struct ConnectOptions {
    /// user is a pair values of name and password
    user: Option<(String, String)>,
    /// HTTP2 keep-alive: (keep_alive_interval, keep_alive_timeout)
    keep_alive: Option<(Duration, Duration)>,
    /// Whether send keep alive pings even there are no active streams.
    keep_alive_while_idle: bool,
    /// Apply a timeout to each gRPC request.
    timeout: Option<Duration>,
    /// Apply a timeout to connecting to the endpoint.
    connect_timeout: Option<Duration>,
    /// TCP keepalive.
    tcp_keepalive: Option<Duration>,
    #[cfg(feature = "tls")]
    tls: Option<TlsOptions>,
    #[cfg(feature = "tls-openssl")]
    otls: Option<OpenSslResult<OpenSslConnector>>,
    /// Require a leader to be present for the operation to complete.
    require_leader: bool,
}

impl ConnectOptions {
    /// name is the identifier for the distributed shared lock to be acquired.
    #[inline]
    pub fn with_user(mut self, name: impl Into<String>, password: impl Into<String>) -> Self {
        self.user = Some((name.into(), password.into()));
        self
    }

    /// Sets TLS options.
    ///
    /// Notes that this function have to work with `HTTPS` URLs.
    #[cfg_attr(docsrs, doc(cfg(feature = "tls")))]
    #[cfg(feature = "tls")]
    #[inline]
    pub fn with_tls(mut self, tls: TlsOptions) -> Self {
        self.tls = Some(tls);
        self
    }

    /// Sets TLS options, however using the OpenSSL implementation.
    #[cfg_attr(docsrs, doc(cfg(feature = "tls-openssl")))]
    #[cfg(feature = "tls-openssl")]
    #[inline]
    pub fn with_openssl_tls(mut self, otls: OpenSslClientConfig) -> Self {
        // NOTE1: Perhaps we can unify the essential TLS config terms by something like `TlsBuilder`?
        //
        // NOTE2: we delay the checking at connection step to keep consistency with tonic, however would
        // things be better if we validate the config at here?
        self.otls = Some(otls.build());
        self
    }

    /// Enable HTTP2 keep-alive with `interval` and `timeout`.
    #[inline]
    pub fn with_keep_alive(mut self, interval: Duration, timeout: Duration) -> Self {
        self.keep_alive = Some((interval, timeout));
        self
    }

    /// Apply a timeout to each request.
    #[inline]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Apply a timeout to connecting to the endpoint.
    #[inline]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Enable TCP keepalive.
    #[inline]
    pub fn with_tcp_keepalive(mut self, tcp_keepalive: Duration) -> Self {
        self.tcp_keepalive = Some(tcp_keepalive);
        self
    }

    /// Whether send keep alive pings even there are no active requests.
    /// If disabled, keep-alive pings are only sent while there are opened request/response streams.
    /// If enabled, pings are also sent when no streams are active.
    /// NOTE: Some implementations of gRPC server may send GOAWAY if there are too many pings.
    ///       This would be useful if you meet some error like `too many pings`.
    #[inline]
    pub fn with_keep_alive_while_idle(mut self, enabled: bool) -> Self {
        self.keep_alive_while_idle = enabled;
        self
    }

    /// Whether to enforce that a leader be present in the etcd cluster.
    #[inline]
    pub fn with_require_leader(mut self, require_leader: bool) -> Self {
        self.require_leader = require_leader;
        self
    }

    /// Creates a `ConnectOptions`.
    #[inline]
    pub const fn new() -> Self {
        ConnectOptions {
            user: None,
            keep_alive: None,
            keep_alive_while_idle: true,
            timeout: None,
            connect_timeout: None,
            tcp_keepalive: None,
            #[cfg(feature = "tls")]
            tls: None,
            #[cfg(feature = "tls-openssl")]
            otls: None,
            require_leader: false,
        }
    }
}
