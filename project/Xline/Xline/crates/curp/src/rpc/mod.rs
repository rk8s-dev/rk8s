use std::{collections::HashMap, sync::Arc};

pub use self::proto::commandpb::CurpError as CurpErrorWrapper;
pub use self::proto::commandpb::{
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
};
pub(crate) use self::proto::inner_messagepb::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    TriggerShutdownRequest, TriggerShutdownResponse, TryBecomeLeaderNowRequest,
    TryBecomeLeaderNowResponse, VoteRequest, VoteResponse,
};
use crate::{LogIndex, cmd::Command, log_entry::LogEntry, members::ServerId};
use async_trait::async_trait;
use curp_external_api::{
    InflightId,
    cmd::{ConflictCheck, PbCodec, PbSerializeError},
    conflict::EntryId,
};
use futures::Stream;
use prost::Message;
use serde::{Deserialize, Serialize};
use xlinerpc::status::{Code, Status};

/// Metrics
#[cfg(feature = "client-metrics")]
mod metrics;

/// Rpc connect
pub(crate) mod connect;
pub(crate) use connect::{quic_connect, quic_connects, quic_inner_connects};

/// Transport configuration
pub(crate) mod transport;
#[allow(unused_imports)]
pub use transport::TransportConfig;

/// QUIC transport implementation
pub(crate) mod quic_transport;

pub use quic_transport::{DnsFallback, MethodId, QuicChannel, QuicGrpcServer};

#[doc(hidden)]
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
/// Server side: read bypass/token directly from Metadata, and rebuild tracing context
///              via OpenTelemetry Extractor.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    /// Key-value pairs
    pairs: Vec<(String, String)>,
}

impl Metadata {
    /// Create a new empty metadata
    #[inline]
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Insert a key-value pair
    #[inline]
    #[allow(dead_code)]
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.pairs.push((key.into(), value.into()));
    }

    /// Get value by key (last-wins semantics for duplicate keys)
    #[inline]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .rfind(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Check if request is bypassed
    #[inline]
    pub fn is_bypassed(&self) -> bool {
        self.get("bypass") == Some("true")
    }

    /// Get auth token
    #[inline]
    #[allow(dead_code)]
    pub fn token(&self) -> Option<&str> {
        self.get("token")
    }

    /// Iterate over all key-value pairs (for serialization)
    #[inline]
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Rebuild from deserialized key-value pairs
    #[inline]
    pub fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        Self { pairs }
    }
}

// ============================================================================
// Extract / Inject impls for Metadata
//
// These live in curp crate (not utils) because Metadata is defined here.
// OpenTelemetry propagator uses the Extractor/Injector traits to read/write
// W3C Trace Context headers (traceparent, tracestate) from/to Metadata.
// ============================================================================

/// Adapter for OpenTelemetry Extractor trait
struct MetadataExtractor<'a>(&'a Metadata);

impl opentelemetry::propagation::Extractor for MetadataExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key)
    }

    fn keys(&self) -> Vec<&str> {
        self.0.pairs.iter().map(|(k, _)| k.as_str()).collect()
    }
}

/// Adapter for OpenTelemetry Injector trait
struct MetadataInjector<'a>(&'a mut Metadata);

impl opentelemetry::propagation::Injector for MetadataInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key, value);
    }
}

impl utils::tracing::Extract for Metadata {
    #[inline]
    fn extract_span(&self) {
        let parent_ctx = opentelemetry::global::get_text_map_propagator(|prop| {
            prop.extract(&MetadataExtractor(self))
        });
        let span = tracing::Span::current();
        tracing_opentelemetry::OpenTelemetrySpanExt::set_parent(&span, parent_ctx);
    }
}

impl utils::tracing::Inject for Metadata {
    #[inline]
    fn inject_span(&mut self, span: &tracing::Span) {
        let ctx = tracing_opentelemetry::OpenTelemetrySpanExt::context(span);
        opentelemetry::global::get_text_map_propagator(|prop| {
            prop.inject_context(&ctx, &mut MetadataInjector(self));
        });
    }
}

/// Transport-agnostic service trait for external protocol
///
/// This trait abstracts the RPC methods so that different transport
/// implementations can be used interchangeably by the dispatcher.
#[async_trait]
pub trait CurpService: Send + Sync + 'static {
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
        include!(concat!(env!("OUT_DIR"), "/commandpb.rs"));
    }

    pub(crate) mod inner_messagepb {
        include!(concat!(env!("OUT_DIR"), "/inner_messagepb.rs"));
    }
}