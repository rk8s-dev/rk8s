//! CURP client
//! The Curp client should be used to access the Curp service cluster, instead of using direct RPC.

/// Metrics layer
#[cfg(feature = "client-metrics")]
mod metrics;

/// Unary rpc client
mod unary;

/// Stream rpc client
mod stream;

/// Retry layer
mod retry;

/// State for clients
mod state;

/// Tests for client
#[cfg(test)]
mod tests;

use std::{collections::HashMap, fmt::Debug, ops::Deref, sync::Arc};

use async_trait::async_trait;
use curp_external_api::cmd::Command;
use futures::{StreamExt, stream::FuturesUnordered};
use parking_lot::RwLock;
use tokio::task::JoinHandle;
use tracing::debug;
use utils::config::ClientConfig;
use xlinerpc::status::Status;

use self::{
    retry::{Retry, RetryConfig},
    state::StateBuilder,
    unary::{Unary, UnaryConfig},
};
use crate::{
    members::ServerId,
    rpc::{
        ConfChange, CurpService, FetchClusterRequest, FetchClusterResponse, Member, ProposeId,
        ReadState,
    },
    tracker::Tracker,
};

/// The response of propose command, deserialized from [`crate::rpc::ProposeResponse`] or
/// [`crate::rpc::WaitSyncedResponse`].
#[allow(type_alias_bounds)] // that's not bad
pub(crate) type ProposeResponse<C: Command> = Result<(C::ER, Option<C::ASR>), C::Error>;

/// `ClientApi`, a higher wrapper for `ConnectApi`, providing some methods for communicating to
/// the whole curp cluster. Automatically discovery curp server to update it's quorum.
#[async_trait]
#[allow(clippy::module_name_repetitions)] // better than just Api
pub trait ClientApi {
    /// The client error
    type Error;

    /// The command type
    type Cmd: Command;

    /// Send propose to the whole cluster, `use_fast_path` set to `false` to fallback into ordered
    /// requests (event the requests are commutative).
    async fn propose(
        &self,
        cmd: &Self::Cmd,
        token: Option<&String>, // TODO: Allow external custom interceptors, do not pass token in parameters
        use_fast_path: bool,
    ) -> Result<ProposeResponse<Self::Cmd>, Self::Error>;

    /// Send propose configuration changes to the cluster
    async fn propose_conf_change(
        &self,
        changes: Vec<ConfChange>,
    ) -> Result<Vec<Member>, Self::Error>;

    /// Send propose to shutdown cluster
    async fn propose_shutdown(&self) -> Result<(), Self::Error>;

    /// Send propose to publish a node id and name
    async fn propose_publish(
        &self,
        node_id: ServerId,
        node_name: String,
        node_client_urls: Vec<String>,
    ) -> Result<(), Self::Error>;

    /// Send move leader request
    async fn move_leader(&self, node_id: ServerId) -> Result<(), Self::Error>;

    /// Send fetch read state from leader
    async fn fetch_read_state(&self, cmd: &Self::Cmd) -> Result<ReadState, Self::Error>;

    /// Send fetch cluster requests to all servers (That's because initially, we didn't
    /// know who the leader is.)
    ///
    /// Note: The fetched cluster may still be outdated if `linearizable` is false
    async fn fetch_cluster(&self, linearizable: bool) -> Result<FetchClusterResponse, Self::Error>;

    /// Fetch leader id
    #[inline]
    async fn fetch_leader_id(&self, linearizable: bool) -> Result<ServerId, Self::Error> {
        if linearizable {
            let resp = self.fetch_cluster(true).await?;
            return Ok(resp
                .leader_id
                .unwrap_or_else(|| {
                    unreachable!("linearizable fetch cluster should return a leader id")
                })
                .into());
        }
        let resp = self.fetch_cluster(false).await?;
        if let Some(id) = resp.leader_id {
            return Ok(id.into());
        }
        debug!("no leader id in FetchClusterResponse, try to send linearizable request");
        // fallback to linearizable fetch
        self.fetch_leader_id(true).await
    }
}

/// Propose id guard, used to ensure the sequence of propose id is recorded.
struct ProposeIdGuard<'a> {
    /// The propose id
    propose_id: ProposeId,
    /// The tracker
    tracker: &'a RwLock<Tracker>,
}

impl Deref for ProposeIdGuard<'_> {
    type Target = ProposeId;

    fn deref(&self) -> &Self::Target {
        &self.propose_id
    }
}

impl<'a> ProposeIdGuard<'a> {
    /// Create a new propose id guard
    fn new(tracker: &'a RwLock<Tracker>, propose_id: ProposeId) -> Self {
        Self {
            propose_id,
            tracker,
        }
    }
}

impl Drop for ProposeIdGuard<'_> {
    fn drop(&mut self) {
        let _ig = self.tracker.write().record(self.propose_id.1);
    }
}

/// This trait override some unrepeatable methods in `ClientApi`, and a client with this trait will be able to retry.
#[async_trait]
trait RepeatableClientApi: ClientApi {
    /// Generate a unique propose id during the retry process.
    async fn gen_propose_id(&self) -> Result<ProposeIdGuard<'_>, Self::Error>;

    /// Send propose to the whole cluster, `use_fast_path` set to `false` to fallback into ordered
    /// requests (event the requests are commutative).
    async fn propose(
        &self,
        propose_id: ProposeId,
        cmd: &Self::Cmd,
        token: Option<&String>,
        use_fast_path: bool,
    ) -> Result<ProposeResponse<Self::Cmd>, Self::Error>;

    /// Send propose configuration changes to the cluster
    async fn propose_conf_change(
        &self,
        propose_id: ProposeId,
        changes: Vec<ConfChange>,
    ) -> Result<Vec<Member>, Self::Error>;

    /// Send propose to shutdown cluster
    async fn propose_shutdown(&self, id: ProposeId) -> Result<(), Self::Error>;

    /// Send propose to publish a node id and name
    async fn propose_publish(
        &self,
        propose_id: ProposeId,
        node_id: ServerId,
        node_name: String,
        node_client_urls: Vec<String>,
    ) -> Result<(), Self::Error>;
}

/// Update leader state
#[async_trait]
trait LeaderStateUpdate {
    /// update
    async fn update_leader(&self, leader_id: Option<ServerId>, term: u64) -> bool;
}

/// Client builder to build a client
#[derive(Debug, Clone)]
#[allow(clippy::module_name_repetitions)] // better than just Builder
pub struct ClientBuilder {
    /// initial cluster version
    cluster_version: Option<u64>,
    /// initial cluster members
    all_members: Option<HashMap<ServerId, Vec<String>>>,
    /// is current client send request to raw curp server
    is_raw_curp: bool,
    /// initial leader state
    leader_state: Option<(ServerId, u64)>,
    /// client configuration
    config: ClientConfig,
    /// Transport configuration (QUIC)
    transport: Option<crate::rpc::transport::TransportConfig>,
}

/// A client builder with bypass with local server
#[derive(Debug, Clone)]
#[allow(clippy::module_name_repetitions)] // same as above
pub struct ClientBuilderWithBypass<P: CurpService> {
    /// inner builder
    inner: ClientBuilder,
    /// local server id
    local_server_id: ServerId,
    /// local server
    local_server: P,
}

impl ClientBuilder {
    /// Create a client builder
    #[inline]
    #[must_use]
    pub fn new(config: ClientConfig, is_raw_curp: bool) -> Self {
        Self {
            is_raw_curp,
            config,
            cluster_version: None,
            all_members: None,
            leader_state: None,
            transport: None,
        }
    }

    /// Set the local server to bypass `gRPC` request
    #[inline]
    #[must_use]
    pub fn bypass<P: CurpService>(
        self,
        local_server_id: ServerId,
        local_server: P,
    ) -> ClientBuilderWithBypass<P> {
        ClientBuilderWithBypass {
            inner: self,
            local_server_id,
            local_server,
        }
    }

    /// Set the initial cluster version
    #[inline]
    #[must_use]
    pub fn cluster_version(mut self, cluster_version: u64) -> Self {
        self.cluster_version = Some(cluster_version);
        self
    }

    /// Set the initial all members
    #[inline]
    #[must_use]
    pub fn all_members(mut self, all_members: HashMap<ServerId, Vec<String>>) -> Self {
        self.all_members = Some(all_members);
        self
    }

    /// Set the initial leader state
    #[inline]
    #[must_use]
    pub fn leader_state(mut self, leader_id: ServerId, term: u64) -> Self {
        self.leader_state = Some((leader_id, term));
        self
    }

    /// Use QUIC transport
    #[inline]
    #[must_use]
    pub fn quic_transport(mut self, quic_client: Arc<dquic::prelude::QuicClient>) -> Self {
        self.transport = Some(crate::rpc::transport::TransportConfig {
            client: quic_client,
            dns_fallback: crate::rpc::quic_transport::channel::DnsFallback::Disabled,
        });
        self
    }

    /// Use QUIC transport with localhost DNS fallback (test only)
    ///
    /// When DNS resolution fails for a hostname, falls back to 127.0.0.1
    /// with the original hostname as SNI. Only use this for testing with
    /// fake hostnames like "s0.test".
    #[doc(hidden)]
    #[inline]
    #[must_use]
    pub fn quic_transport_for_test(mut self, quic_client: Arc<dquic::prelude::QuicClient>) -> Self {
        self.transport = Some(crate::rpc::transport::TransportConfig {
            client: quic_client,
            dns_fallback: crate::rpc::quic_transport::channel::DnsFallback::LocalhostForTest,
        });
        self
    }

    /// Discover the initial states from some endpoints
    ///
    /// # Errors
    ///
    /// Return `CurpError` for connection failure or some server errors.
    #[inline]
    pub async fn discover_from(
        mut self,
        addrs: Vec<String>,
    ) -> Result<Self, crate::rpc::CurpError> {
        use crate::rpc::{CurpError, quic_transport::channel::QuicChannel};
        use xlinerpc::MethodId;

        let transport = self.transport.as_ref().ok_or_else(|| {
            CurpError::internal("discover_from requires quic_transport to be set")
        })?;
        let quic_client = Arc::clone(&transport.client);
        let dns_fallback = transport.dns_fallback;

        let propose_timeout = *self.config.propose_timeout();
        let mut futs: FuturesUnordered<_> = addrs
            .iter()
            .map(|addr| {
                let client = Arc::clone(&quic_client);
                let addr = addr.clone();
                async move {
                    let channel = QuicChannel::with_addrs(client, vec![addr], dns_fallback);
                    let resp: FetchClusterResponse = channel
                        .unary_call(
                            MethodId::FetchCluster,
                            FetchClusterRequest::default(),
                            vec![],
                            propose_timeout,
                        )
                        .await?;
                    Ok::<FetchClusterResponse, CurpError>(resp)
                }
            })
            .collect();

        let mut err = CurpError::internal("addrs is empty");
        while let Some(r) = futs.next().await {
            match r {
                Ok(r) => {
                    self.cluster_version = Some(r.cluster_version);
                    if let Some(ref id) = r.leader_id {
                        self.leader_state = Some((id.into(), r.term));
                    }
                    self.all_members = if self.is_raw_curp {
                        Some(r.into_peer_urls())
                    } else {
                        Some(Self::ensure_no_empty_address(r.into_client_urls())?)
                    };
                    return Ok(self);
                }
                Err(e) => err = e,
            }
        }
        Err(err)
    }

    /// Ensures that no server has an empty list of addresses
    fn ensure_no_empty_address(
        urls: HashMap<ServerId, Vec<String>>,
    ) -> Result<HashMap<ServerId, Vec<String>>, crate::rpc::CurpError> {
        (!urls.values().any(Vec::is_empty))
            .then_some(urls)
            .ok_or(crate::rpc::CurpError::internal("cluster not published"))
    }

    /// Init state builder
    fn init_state_builder(&self) -> StateBuilder {
        let transport = self
            .transport
            .clone()
            .unwrap_or_else(|| panic!("transport must be set before building client"));
        let mut builder = StateBuilder::new(
            self.all_members.clone().unwrap_or_else(|| {
                unreachable!("must set the initial members or discover from some endpoints")
            }),
            transport,
        );
        if let Some(version) = self.cluster_version {
            builder.set_cluster_version(version);
        }
        if let Some((id, term)) = self.leader_state {
            builder.set_leader_state(id, term);
        }
        builder.set_is_raw_curp(self.is_raw_curp);
        builder
    }

    /// Init retry config
    fn init_retry_config(&self) -> RetryConfig {
        if *self.config.fixed_backoff() {
            RetryConfig::new_fixed(
                *self.config.initial_retry_timeout(),
                *self.config.retry_count(),
            )
        } else {
            RetryConfig::new_exponential(
                *self.config.initial_retry_timeout(),
                *self.config.max_retry_timeout(),
                *self.config.retry_count(),
            )
        }
    }

    /// Init unary config
    fn init_unary_config(&self) -> UnaryConfig {
        UnaryConfig::new(
            *self.config.propose_timeout(),
            *self.config.wait_synced_timeout(),
        )
    }

    /// Spawn background tasks for the client
    fn spawn_bg_tasks(&self, state: Arc<state::State>) -> JoinHandle<()> {
        let interval = *self.config.keep_alive_interval();
        tokio::spawn(async move {
            let stream = stream::Streaming::new(state, stream::StreamingConfig::new(interval));
            stream.keep_heartbeat().await;
            debug!("keep heartbeat task shutdown");
        })
    }

    /// Build the client
    ///
    /// # Errors
    ///
    /// Return error for connection failure.
    #[inline]
    #[allow(clippy::result_large_err)]
    pub fn build<C: Command>(
        &self,
    ) -> Result<impl ClientApi<Error = Status, Cmd = C> + Send + Sync + 'static + use<C>, Status>
    {
        let state = Arc::new(self.init_state_builder().build());
        let client = Retry::new(
            Unary::new(Arc::clone(&state), self.init_unary_config()),
            self.init_retry_config(),
            Some(self.spawn_bg_tasks(Arc::clone(&state))),
        );

        Ok(client)
    }
}

impl<P: CurpService> ClientBuilderWithBypass<P> {
    /// Build the client with local server
    ///
    /// # Errors
    ///
    /// Return error for connection failure.
    #[inline]
    #[allow(clippy::result_large_err)]
    pub fn build<C: Command>(self) -> Result<impl ClientApi<Error = Status, Cmd = C>, Status> {
        let state = self
            .inner
            .init_state_builder()
            .build_bypassed::<P>(self.local_server_id, self.local_server);
        let state = Arc::new(state);
        let client = Retry::new(
            Unary::new(Arc::clone(&state), self.inner.init_unary_config()),
            self.inner.init_retry_config(),
            Some(self.inner.spawn_bg_tasks(Arc::clone(&state))),
        );

        Ok(client)
    }
}
