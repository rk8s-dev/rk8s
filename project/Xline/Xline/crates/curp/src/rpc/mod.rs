use std::{collections::HashMap, sync::Arc};

#[cfg(feature = "quic")]
use async_trait::async_trait;
use curp_external_api::{
    InflightId,
    cmd::{ConflictCheck, PbCodec, PbSerializeError},
    conflict::EntryId,
};
#[cfg(feature = "quic")]
use futures::Stream;
use prost::Message;
use serde::{Deserialize, Serialize};
use tonic::{Code, Status};
// TODO: use our own status type
// use xlinerpc::status::{Code,Status};
pub(crate) use self::proto::{
    commandpb::CurpError as CurpErrorWrapper,
    inner_messagepb::{
        AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest,
        InstallSnapshotResponse, TriggerShutdownRequest, TriggerShutdownResponse,
        TryBecomeLeaderNowRequest, TryBecomeLeaderNowResponse, VoteRequest, VoteResponse,
        inner_protocol_server::InnerProtocol,
    },
};
pub use self::proto::{
    commandpb::{
        CmdResult,
        FetchClusterRequest,
        FetchClusterResponse,
        FetchReadStateRequest,
        FetchReadStateResponse,
        LeaseKeepAliveMsg,
        Member,
        MoveLeaderRequest,
        MoveLeaderResponse,
        OpResponse,
        OptionalU64,
        ProposeConfChangeRequest,
        ProposeConfChangeResponse,
        ProposeId as PbProposeId,
        ProposeRequest,
        ProposeResponse,
        PublishRequest,
        PublishResponse,
        ReadIndexRequest,
        ReadIndexResponse,
        RecordRequest,
        RecordResponse,
        ShutdownRequest,
        ShutdownResponse,
        SyncedResponse,
        WaitSyncedRequest,
        WaitSyncedResponse,
        cmd_result::Result as CmdResultInner,
        curp_error::Err as CurpError, // easy for match
        curp_error::Redirect,
        fetch_read_state_response::{IdSet, ReadState},
        op_response::Op as ResponseOp,
        propose_conf_change_request::{ConfChange, ConfChangeType},
        protocol_client,
        protocol_server::{Protocol, ProtocolServer},
    },
    inner_messagepb::inner_protocol_server::InnerProtocolServer,
};
use crate::{LogIndex, cmd::Command, log_entry::LogEntry, members::ServerId};

/// Metrics
#[cfg(feature = "client-metrics")]
mod metrics;

/// Rpc connect
pub(crate) mod connect;
pub(crate) use connect::{connect, connects, inner_connects};

#[cfg(feature = "quic")]
#[allow(unused_imports)]
pub(crate) use connect::{quic_connect, quic_connects, quic_inner_connects};

/// Auto reconnect connection
mod reconnect;

/// Transport configuration
pub(crate) mod transport;
#[allow(unused_imports)]
pub(crate) use transport::TransportConfig;

/// QUIC transport implementation
#[cfg(feature = "quic")]
pub(crate) mod quic_transport;

#[cfg(feature = "quic")]
pub use quic_transport::{DnsFallback, MethodId, QuicChannel, QuicGrpcServer};

#[doc(hidden)]
#[cfg(all(feature = "quic", any(test, feature = "quic-test")))]
pub use quic_transport::ALL_METHOD_IDS;

// ============================================================================
// Transport-agnostic service traits
// ============================================================================

/// Generic key-value metadata container
///
/// Carries three types of data:
/// 1. bypass flag — key="bypass", value="true"
/// 2. auth token — key="token", value=<jwt>
/// 3. tracing context — W3C Trace Context keys (traceparent, tracestate, etc.),
///    dynamically injected by OpenTelemetry Propagator
///
/// Client side: inject tracing context into Metadata, then serialize to QUIC frame header.
/// Server side: rebuild `tonic::metadata::MetadataMap` from Metadata for `extract_span()`,
///              and directly read bypass/token.
#[cfg(feature = "quic")]
#[derive(Debug, Clone, Default)]
pub(crate) struct Metadata {
    /// Key-value pairs
    pairs: Vec<(String, String)>,
}

#[cfg(feature = "quic")]
impl Metadata {
    /// Create a new empty metadata
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Insert a key-value pair
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.pairs.push((key.into(), value.into()));
    }

    /// Get value by key (last-wins semantics for duplicate keys)
    #[inline]
    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .rfind(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Check if request is bypassed
    #[inline]
    pub(crate) fn is_bypassed(&self) -> bool {
        self.get("bypass").is_some()
    }

    /// Get auth token
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn token(&self) -> Option<&str> {
        self.get("token")
    }

    /// Iterate over all key-value pairs (for serialization)
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Rebuild from deserialized key-value pairs
    #[inline]
    pub(crate) fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        Self { pairs }
    }

    /// Convert to `tonic::metadata::MetadataMap` (for server-side `extract_span`)
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn to_metadata_map(&self) -> tonic::metadata::MetadataMap {
        use tonic::metadata::{MetadataKey, MetadataValue};

        let mut map = tonic::metadata::MetadataMap::new();
        for (k, v) in &self.pairs {
            if let (Ok(key), Ok(val)) = (
                k.parse::<MetadataKey<tonic::metadata::Ascii>>(),
                v.parse::<MetadataValue<tonic::metadata::Ascii>>(),
            ) {
                let _ig = map.insert(key, val);
            }
        }
        map
    }
}

/// Transport-agnostic service trait for external protocol
///
/// This trait abstracts the RPC methods so that both tonic and QUIC
/// implementations can be used interchangeably by the dispatcher.
#[cfg(feature = "quic")]
#[async_trait]
pub(crate) trait CurpService: Send + Sync + 'static {
    /// Handle propose stream request
    async fn propose_stream(
        &self,
        req: ProposeRequest,
        meta: Metadata,
    ) -> Result<Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send + Unpin>, CurpError>;

    /// Handle record request
    fn record(&self, req: RecordRequest, meta: Metadata) -> Result<RecordResponse, CurpError>;

    /// Handle read index request
    fn read_index(&self, meta: Metadata) -> Result<ReadIndexResponse, CurpError>;

    /// Handle shutdown request
    async fn shutdown(
        &self,
        req: ShutdownRequest,
        meta: Metadata,
    ) -> Result<ShutdownResponse, CurpError>;

    /// Handle propose conf change request
    async fn propose_conf_change(
        &self,
        req: ProposeConfChangeRequest,
        meta: Metadata,
    ) -> Result<ProposeConfChangeResponse, CurpError>;

    /// Handle publish request
    fn publish(&self, req: PublishRequest, meta: Metadata) -> Result<PublishResponse, CurpError>;

    /// Handle fetch cluster request
    fn fetch_cluster(&self, req: FetchClusterRequest) -> Result<FetchClusterResponse, CurpError>;

    /// Handle fetch read state request
    fn fetch_read_state(
        &self,
        req: FetchReadStateRequest,
    ) -> Result<FetchReadStateResponse, CurpError>;

    /// Handle move leader request
    async fn move_leader(&self, req: MoveLeaderRequest) -> Result<MoveLeaderResponse, CurpError>;

    /// Handle lease keep alive stream
    async fn lease_keep_alive(
        &self,
        stream: Box<dyn Stream<Item = Result<LeaseKeepAliveMsg, CurpError>> + Send + Unpin>,
    ) -> Result<LeaseKeepAliveMsg, CurpError>;
}

/// Transport-agnostic service trait for internal protocol
///
/// This trait abstracts the internal RPC methods used for Raft consensus.
#[cfg(feature = "quic")]
#[async_trait]
pub(crate) trait InnerCurpService: Send + Sync + 'static {
    /// Handle append entries request
    fn append_entries(&self, req: AppendEntriesRequest)
    -> Result<AppendEntriesResponse, CurpError>;

    /// Handle vote request
    fn vote(&self, req: VoteRequest) -> Result<VoteResponse, CurpError>;

    /// Handle install snapshot stream
    async fn install_snapshot(
        &self,
        stream: Box<dyn Stream<Item = Result<InstallSnapshotRequest, CurpError>> + Send + Unpin>,
    ) -> Result<InstallSnapshotResponse, CurpError>;

    /// Trigger shutdown
    fn trigger_shutdown(&self) -> Result<(), CurpError>;

    /// Try to become leader now
    async fn try_become_leader_now(&self) -> Result<(), CurpError>;
}

// Skip for generated code
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
mod proto {
    pub(crate) mod commandpb {
        tonic::include_proto!("commandpb");
    }

    pub(crate) mod inner_messagepb {
        tonic::include_proto!("inner_messagepb");
    }
}

impl From<PbProposeId> for ProposeId {
    #[inline]
    fn from(id: PbProposeId) -> Self {
        Self(id.client_id, id.seq_num)
    }
}

impl From<ProposeId> for PbProposeId {
    #[inline]
    fn from(id: ProposeId) -> Self {
        Self {
            client_id: id.0,
            seq_num: id.1,
        }
    }
}

impl From<u64> for OptionalU64 {
    #[inline]
    fn from(value: u64) -> Self {
        Self { value }
    }
}

impl From<OptionalU64> for u64 {
    #[inline]
    fn from(value: OptionalU64) -> Self {
        value.value
    }
}

impl From<&OptionalU64> for u64 {
    #[inline]
    fn from(value: &OptionalU64) -> Self {
        value.value
    }
}

impl FetchClusterResponse {
    /// Create a new `FetchClusterResponse`
    pub(crate) fn new(
        leader_id: Option<ServerId>,
        term: u64,
        cluster_id: u64,
        members: Vec<Member>,
        cluster_version: u64,
    ) -> Self {
        Self {
            leader_id: leader_id.map(Into::into),
            term,
            cluster_id,
            members,
            cluster_version,
        }
    }

    /// Get all members peer urls
    pub(crate) fn into_peer_urls(self) -> HashMap<ServerId, Vec<String>> {
        self.members
            .into_iter()
            .map(|member| (member.id, member.peer_urls))
            .collect()
    }

    /// Get all members peer urls
    pub(crate) fn into_client_urls(self) -> HashMap<ServerId, Vec<String>> {
        self.members
            .into_iter()
            .map(|member| (member.id, member.client_urls))
            .collect()
    }
}

impl ProposeRequest {
    /// Create a new `Propose` request
    #[inline]
    pub fn new<C: Command>(
        propose_id: ProposeId,
        cmd: &C,
        cluster_version: u64,
        term: u64,
        slow_path: bool,
        first_incomplete: u64,
    ) -> Self {
        Self {
            propose_id: Some(propose_id.into()),
            command: cmd.encode(),
            cluster_version,
            term,
            slow_path,
            first_incomplete,
        }
    }

    /// Get the propose id
    #[inline]
    #[must_use]
    pub fn propose_id(&self) -> ProposeId {
        self.propose_id
            .unwrap_or_else(|| unreachable!("propose id must be set in ProposeRequest"))
            .into()
    }

    /// Get command
    ///
    /// # Errors
    ///
    /// Return error if the command can't be decoded
    #[inline]
    pub fn cmd<C: Command>(&self) -> Result<C, PbSerializeError> {
        C::decode(&self.command)
    }
}

impl ProposeResponse {
    /// Create an ok propose response
    pub(crate) fn new_result<C: Command>(result: &Result<C::ER, C::Error>, conflict: bool) -> Self {
        let result = match *result {
            Ok(ref er) => Some(CmdResult {
                result: Some(CmdResultInner::Ok(er.encode())),
            }),
            Err(ref e) => Some(CmdResult {
                result: Some(CmdResultInner::Error(e.encode())),
            }),
        };
        Self { result, conflict }
    }

    /// Create an empty propose response
    #[allow(unused)]
    pub(crate) fn new_empty() -> Self {
        Self {
            result: None,
            conflict: false,
        }
    }

    /// Deserialize result in response and take a map function
    pub(crate) fn map_result<C: Command, F, R>(self, f: F) -> Result<R, PbSerializeError>
    where
        F: FnOnce(Result<Option<C::ER>, C::Error>) -> R,
    {
        let Some(res) = self.result.and_then(|res| res.result) else {
            return Ok(f(Ok(None)));
        };
        let res = match res {
            CmdResultInner::Ok(ref buf) => Ok(<C as Command>::ER::decode(buf)?),
            CmdResultInner::Error(ref buf) => Err(<C as Command>::Error::decode(buf)?),
        };
        Ok(f(res.map(Some)))
    }
}

impl RecordRequest {
    /// Creates a new `RecordRequest`
    pub(crate) fn new<C: Command>(propose_id: ProposeId, command: &C) -> Self {
        RecordRequest {
            propose_id: Some(propose_id.into()),
            command: command.encode(),
        }
    }

    /// Get the propose id
    pub(crate) fn propose_id(&self) -> ProposeId {
        self.propose_id
            .unwrap_or_else(|| {
                unreachable!("propose id should be set in propose wait synced request")
            })
            .into()
    }

    /// Get command
    pub(crate) fn cmd<C: Command>(&self) -> Result<C, PbSerializeError> {
        C::decode(&self.command)
    }
}

impl SyncedResponse {
    /// Create a new response from `after_sync` result
    pub(crate) fn new_result<C: Command>(result: &Result<C::ASR, C::Error>) -> Self {
        match *result {
            Ok(ref asr) => SyncedResponse {
                after_sync_result: Some(CmdResult {
                    result: Some(CmdResultInner::Ok(asr.encode())),
                }),
            },
            Err(ref e) => SyncedResponse {
                after_sync_result: Some(CmdResult {
                    result: Some(CmdResultInner::Error(e.encode())),
                }),
            },
        }
    }

    /// Deserialize result in response and take a map function
    pub(crate) fn map_result<C: Command, F, R>(self, f: F) -> Result<R, PbSerializeError>
    where
        F: FnOnce(Option<Result<C::ASR, C::Error>>) -> R,
    {
        let Some(res) = self.after_sync_result.and_then(|res| res.result) else {
            return Ok(f(None));
        };
        let res = match res {
            CmdResultInner::Ok(ref buf) => Ok(<C as Command>::ASR::decode(buf)?),
            CmdResultInner::Error(ref buf) => Err(<C as Command>::Error::decode(buf)?),
        };
        Ok(f(Some(res)))
    }
}

impl AppendEntriesRequest {
    /// Create a new `append_entries` request
    pub(crate) fn new<C: Command>(
        term: u64,
        leader_id: ServerId,
        prev_log_index: LogIndex,
        prev_log_term: u64,
        entries: Vec<Arc<LogEntry<C>>>,
        leader_commit: LogIndex,
    ) -> bincode::Result<Self> {
        Ok(Self {
            term,
            leader_id,
            prev_log_index,
            prev_log_term,
            entries: entries
                .into_iter()
                .map(|e| bincode::serialize(&e))
                .collect::<bincode::Result<Vec<Vec<u8>>>>()?,
            leader_commit,
        })
    }

    /// Get log entries
    pub(crate) fn entries<C: Command>(&self) -> bincode::Result<Vec<LogEntry<C>>> {
        self.entries
            .iter()
            .map(|entry| bincode::deserialize(entry))
            .collect()
    }
}

impl AppendEntriesResponse {
    /// Create a new rejected response
    pub(crate) fn new_reject(term: u64, hint_index: LogIndex) -> Self {
        Self {
            term,
            success: false,
            hint_index,
        }
    }

    /// Create a new accepted response
    pub(crate) fn new_accept(term: u64) -> Self {
        Self {
            term,
            success: true,
            hint_index: 0,
        }
    }
}

impl VoteRequest {
    /// Create a new vote request
    pub(crate) fn new(
        term: u64,
        candidate_id: ServerId,
        last_log_index: LogIndex,
        last_log_term: u64,
        is_pre_vote: bool,
    ) -> Self {
        Self {
            term,
            candidate_id,
            last_log_index,
            last_log_term,
            is_pre_vote,
        }
    }
}

impl VoteResponse {
    /// Create a new accepted vote response
    pub(crate) fn new_accept<C: Command>(
        term: u64,
        cmds: Vec<PoolEntry<C>>,
    ) -> bincode::Result<Self> {
        Ok(Self {
            term,
            vote_granted: true,
            spec_pool: cmds
                .into_iter()
                .map(|c| bincode::serialize(&c))
                .collect::<bincode::Result<Vec<Vec<u8>>>>()?,
            shutdown_candidate: false,
        })
    }

    /// Create a new rejected vote response
    pub(crate) fn new_reject(term: u64) -> Self {
        Self {
            term,
            vote_granted: false,
            spec_pool: vec![],
            shutdown_candidate: false,
        }
    }

    /// Create a new shutdown vote response
    pub(crate) fn new_shutdown() -> Self {
        Self {
            term: 0,
            vote_granted: false,
            spec_pool: vec![],
            shutdown_candidate: true,
        }
    }

    /// Get spec pool
    pub(crate) fn spec_pool<C: Command>(&self) -> bincode::Result<Vec<PoolEntry<C>>> {
        self.spec_pool
            .iter()
            .map(|cmd| bincode::deserialize(cmd))
            .collect()
    }
}

impl InstallSnapshotResponse {
    /// Create a new snapshot response
    pub(crate) fn new(term: u64) -> Self {
        Self { term }
    }
}

impl IdSet {
    /// Create a new `IdSet`
    pub(crate) fn new(inflight_ids: Vec<InflightId>) -> Self {
        Self { inflight_ids }
    }
}

impl FetchReadStateRequest {
    /// Create a new fetch read state request
    pub(crate) fn new<C: Command>(cmd: &C, cluster_version: u64) -> bincode::Result<Self> {
        Ok(Self {
            command: bincode::serialize(cmd)?,
            cluster_version,
        })
    }

    /// Get command
    pub(crate) fn cmd<C: Command>(&self) -> bincode::Result<C> {
        bincode::deserialize(&self.command)
    }
}

impl FetchReadStateResponse {
    /// Create a new fetch read state response
    pub(crate) fn new(state: ReadState) -> Self {
        Self {
            read_state: Some(state),
        }
    }
}

#[allow(clippy::as_conversions)] // ConfChangeType is so small that it won't exceed the range of i32 type.
impl ConfChange {
    /// Create a new `ConfChange` to add a node
    #[must_use]
    #[inline]
    pub fn add(node_id: ServerId, address: Vec<String>) -> Self {
        Self {
            change_type: ConfChangeType::Add as i32,
            node_id,
            address,
        }
    }

    /// Create a new `ConfChange` to remove a node
    #[must_use]
    #[inline]
    pub fn remove(node_id: ServerId) -> Self {
        Self {
            change_type: ConfChangeType::Remove as i32,
            node_id,
            address: vec![],
        }
    }

    /// Create a new `ConfChange` to update a node
    #[must_use]
    #[inline]
    pub fn update(node_id: ServerId, address: Vec<String>) -> Self {
        Self {
            change_type: ConfChangeType::Update as i32,
            node_id,
            address,
        }
    }

    /// Create a new `ConfChange` to add a learner node
    #[must_use]
    #[inline]
    pub fn add_learner(node_id: ServerId, address: Vec<String>) -> Self {
        Self {
            change_type: ConfChangeType::AddLearner as i32,
            node_id,
            address,
        }
    }

    /// Create a new `ConfChange` to promote a learner node
    #[must_use]
    #[inline]
    pub fn promote_learner(node_id: ServerId) -> Self {
        Self {
            change_type: ConfChangeType::Promote as i32,
            node_id,
            address: vec![],
        }
    }

    /// Create a new `ConfChange` to promote a node
    #[must_use]
    #[inline]
    pub fn promote(node_id: ServerId) -> Self {
        Self {
            change_type: ConfChangeType::Promote as i32,
            node_id,
            address: vec![],
        }
    }
}

impl ProposeConfChangeRequest {
    /// Create a new `ProposeConfChangeRequest`
    pub(crate) fn new(id: ProposeId, changes: Vec<ConfChange>, cluster_version: u64) -> Self {
        Self {
            propose_id: Some(id.into()),
            changes,
            cluster_version,
        }
    }

    /// Get id of the request
    pub(crate) fn propose_id(&self) -> ProposeId {
        self.propose_id
            .unwrap_or_else(|| {
                unreachable!("propose id should be set in propose conf change request")
            })
            .into()
    }
}

impl ShutdownRequest {
    /// Create a new shutdown request
    pub(crate) fn new(id: ProposeId, cluster_version: u64) -> Self {
        Self {
            propose_id: Some(id.into()),
            cluster_version,
        }
    }

    /// Get id of the request
    pub(crate) fn propose_id(&self) -> ProposeId {
        self.propose_id
            .unwrap_or_else(|| {
                unreachable!("propose id should be set in propose conf change request")
            })
            .into()
    }
}

impl MoveLeaderRequest {
    /// Create a new `MoveLeaderRequest`
    pub(crate) fn new(node_id: ServerId, cluster_version: u64) -> Self {
        Self {
            node_id,
            cluster_version,
        }
    }
}

impl PublishRequest {
    /// Create a new `PublishRequest`
    pub(crate) fn new(
        id: ProposeId,
        node_id: ServerId,
        name: String,
        client_urls: Vec<String>,
    ) -> Self {
        Self {
            propose_id: Some(id.into()),
            node_id,
            name,
            client_urls,
        }
    }

    /// Get id of the request
    pub(crate) fn propose_id(&self) -> ProposeId {
        self.propose_id
            .unwrap_or_else(|| {
                unreachable!("propose id should be set in propose conf change request")
            })
            .into()
    }
}

/// NOTICE:
///
/// Please check test case `test_unary_fast_round_return_early_err`
/// `test_unary_propose_return_early_err`
/// `test_retry_propose_return_no_retry_error`
/// `test_retry_propose_return_retry_error` if you added some new [`CurpError`]
impl CurpError {
    /// `Duplicated` error
    #[allow(unused)]
    pub(crate) fn duplicated() -> Self {
        Self::Duplicated(())
    }

    /// `ExpiredClientId` error
    #[allow(unused)] // TODO: used in dedup
    pub(crate) fn expired_client_id() -> Self {
        Self::ExpiredClientId(())
    }

    /// `InvalidConfig` error
    pub(crate) fn invalid_config() -> Self {
        Self::InvalidConfig(())
    }

    /// `NodeNotExists` error
    pub(crate) fn node_not_exist() -> Self {
        Self::NodeNotExists(())
    }

    /// `NodeAlreadyExists` error
    pub(crate) fn node_already_exists() -> Self {
        Self::NodeAlreadyExists(())
    }

    /// `LearnerNotCatchUp` error
    pub(crate) fn learner_not_catch_up() -> Self {
        Self::LearnerNotCatchUp(())
    }

    /// `ShuttingDown` error
    pub(crate) fn shutting_down() -> Self {
        Self::ShuttingDown(())
    }

    /// `Duplicated` error
    pub(crate) fn wrong_cluster_version() -> Self {
        Self::WrongClusterVersion(())
    }

    /// `Redirect` error
    pub(crate) fn redirect(leader_id: Option<ServerId>, term: u64) -> Self {
        Self::Redirect(Redirect {
            leader_id: leader_id.map(Into::into),
            term,
        })
    }

    /// `Internal` error
    pub(crate) fn internal(reason: impl Into<String>) -> Self {
        Self::Internal(reason.into())
    }

    /// Whether to abort fast round early
    pub(crate) fn should_abort_fast_round(&self) -> bool {
        matches!(
            *self,
            CurpError::Duplicated(())
                | CurpError::ShuttingDown(())
                | CurpError::InvalidConfig(())
                | CurpError::NodeAlreadyExists(())
                | CurpError::NodeNotExists(())
                | CurpError::LearnerNotCatchUp(())
                | CurpError::ExpiredClientId(())
                | CurpError::Redirect(_)
        )
    }

    /// Whether to abort slow round early
    #[allow(unused)]
    pub(crate) fn should_abort_slow_round(&self) -> bool {
        matches!(
            *self,
            CurpError::ShuttingDown(())
                | CurpError::InvalidConfig(())
                | CurpError::NodeAlreadyExists(())
                | CurpError::NodeNotExists(())
                | CurpError::LearnerNotCatchUp(())
                | CurpError::ExpiredClientId(())
                | CurpError::Redirect(_)
                | CurpError::WrongClusterVersion(())
        )
    }

    /// Get the priority of the error
    pub(crate) fn priority(&self) -> CurpErrorPriority {
        match *self {
            CurpError::Duplicated(())
            | CurpError::ShuttingDown(())
            | CurpError::InvalidConfig(())
            | CurpError::NodeAlreadyExists(())
            | CurpError::NodeNotExists(())
            | CurpError::LearnerNotCatchUp(())
            | CurpError::ExpiredClientId(())
            | CurpError::Redirect(_)
            | CurpError::WrongClusterVersion(())
            | CurpError::Zombie(()) => CurpErrorPriority::High,
            CurpError::RpcTransport(())
            | CurpError::Internal(_)
            | CurpError::KeyConflict(())
            | CurpError::LeaderTransfer(_) => CurpErrorPriority::Low,
        }
    }

    /// `LeaderTransfer` error
    pub(crate) fn leader_transfer(err: impl Into<String>) -> Self {
        Self::LeaderTransfer(err.into())
    }
}

/// The priority of curp error
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CurpErrorPriority {
    /// Low priority, a low-priority error returned may
    /// be overridden by a higher-priority error.
    Low = 1,
    /// High priority, high-priority errors will override
    /// low-priority errors.
    High = 2,
}

impl<E: std::error::Error + 'static> From<E> for CurpError {
    #[inline]
    fn from(value: E) -> Self {
        let err: &dyn std::error::Error = &value;
        if let Some(status) = err.downcast_ref::<Status>() {
            // Unavailable code often occurs in rpc connection errors,
            // Please DO NOT use this code in CurpError to Status.
            if status.code() == Code::Unavailable {
                return Self::RpcTransport(());
            }
            if !status.details().is_empty() {
                return match CurpErrorWrapper::decode(status.details()) {
                    Ok(e) => e
                        .err
                        .unwrap_or_else(|| unreachable!("err must be set in CurpErrorWrapper")),
                    Err(dec_err) => Self::internal(dec_err.to_string()),
                };
            }
        }
        // Errors that are not created manually by `CurpError::xxx()` are trivial,
        // and errors that need to be known to the client are best created manually.
        Self::internal(value.to_string())
    }
}

impl From<CurpError> for Status {
    #[inline]
    fn from(err: CurpError) -> Self {
        let (code, msg) = match err {
            CurpError::KeyConflict(()) => (
                Code::AlreadyExists,
                "Key conflict error: A key conflict occurred.",
            ),
            CurpError::Duplicated(()) => (
                Code::AlreadyExists,
                "Duplicated error: The request already sent.",
            ),
            CurpError::ExpiredClientId(()) => (
                Code::FailedPrecondition,
                "Expired client ID error: The client ID has expired, we cannot tell if this request is duplicated.",
            ),
            CurpError::InvalidConfig(()) => (
                Code::InvalidArgument,
                "Invalid config error: The provided configuration is invalid.",
            ),
            CurpError::NodeNotExists(()) => (
                Code::NotFound,
                "Node not found error: The specified node does not exist.",
            ),
            CurpError::NodeAlreadyExists(()) => (
                Code::AlreadyExists,
                "Node already exists error: The node already exists.",
            ),
            CurpError::LearnerNotCatchUp(()) => (
                Code::FailedPrecondition,
                "Learner not caught up error: The learner has not caught up.",
            ),
            CurpError::ShuttingDown(()) => (
                Code::FailedPrecondition,
                "Shutting down error: The service is currently shutting down.",
            ),
            CurpError::WrongClusterVersion(()) => (
                Code::FailedPrecondition,
                "Wrong cluster version error: The cluster version is incorrect.",
            ),
            CurpError::Redirect(_) => (
                Code::ResourceExhausted,
                "Redirect error: The request should be redirected to another node.",
            ),
            CurpError::Internal(_) => (
                Code::Internal,
                "Internal error: An internal error occurred.",
            ),
            CurpError::RpcTransport(()) => (Code::Cancelled, "Rpc error: Request cancelled"),
            CurpError::LeaderTransfer(_) => (
                Code::FailedPrecondition,
                "Leader transfer error: A leader transfer error occurred.",
            ),
            CurpError::Zombie(()) => (
                Code::FailedPrecondition,
                "Zombie leader error: The leader is a zombie with outdated term.",
            ),
        };

        let details = CurpErrorWrapper { err: Some(err) }.encode_to_vec();

        Status::with_details(code, msg, details.into())
    }
}

// User defined types

/// Entry of speculative pool
#[derive(Debug, Serialize, Deserialize)]
pub struct PoolEntry<C> {
    /// Propose id
    pub(crate) id: ProposeId,
    /// Inner entry
    pub(crate) cmd: Arc<C>,
}

impl<C> PoolEntry<C> {
    /// Create a new pool entry
    #[inline]
    pub fn new(id: ProposeId, inner: Arc<C>) -> Self {
        Self { id, cmd: inner }
    }
}

impl<C> ConflictCheck for PoolEntry<C>
where
    C: ConflictCheck,
{
    #[inline]
    fn is_conflict(&self, other: &Self) -> bool {
        self.cmd.is_conflict(&other.cmd)
    }
}

impl<C> Clone for PoolEntry<C> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            cmd: Arc::clone(&self.cmd),
        }
    }
}

impl<C> std::ops::Deref for PoolEntry<C> {
    type Target = C;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.cmd
    }
}

impl<C> AsRef<C> for PoolEntry<C> {
    #[inline]
    fn as_ref(&self) -> &C {
        self.cmd.as_ref()
    }
}

impl<C> std::hash::Hash for PoolEntry<C> {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<C> PartialEq for PoolEntry<C> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl<C> Eq for PoolEntry<C> {}

impl<C> PartialOrd for PoolEntry<C> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<C> Ord for PoolEntry<C> {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl<C> EntryId for PoolEntry<C> {
    type Id = ProposeId;

    #[inline]
    fn id(&self) -> Self::Id {
        self.id
    }
}

/// Command Id wrapper, which is used to identify a command
///
/// The underlying data is a tuple of (`client_id`, `seq_num`)
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd, Default,
)]
#[allow(clippy::exhaustive_structs)] // It is exhaustive
pub struct ProposeId(pub u64, pub u64);

impl std::fmt::Display for ProposeId {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.0, self.1)
    }
}
