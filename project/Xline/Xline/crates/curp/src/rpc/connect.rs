use std::{
    fmt::{Debug, Formatter},
    ops::Deref,
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};

use async_stream::stream;
use async_trait::async_trait;
use bytes::BytesMut;
use clippy_utilities::NumericCast;
use engine::SnapshotApi;
use futures::Stream;
#[cfg(test)]
use mockall::automock;
use tracing::{error, instrument};
use utils::tracing::Inject;

use crate::{
    members::ServerId,
    rpc::{
        AppendEntriesRequest, AppendEntriesResponse, CurpError, FetchClusterRequest,
        FetchClusterResponse, FetchReadStateRequest, FetchReadStateResponse,
        InstallSnapshotRequest, InstallSnapshotResponse, MoveLeaderRequest, MoveLeaderResponse,
        OpResponse, ProposeConfChangeRequest, ProposeConfChangeResponse, ProposeRequest,
        PublishRequest, PublishResponse, ShutdownRequest, ShutdownResponse, VoteRequest,
        VoteResponse,
    },
    snapshot::Snapshot,
};

use super::{RecordRequest, RecordResponse, proto::commandpb::ReadIndexResponse};

/// Install snapshot chunk size: 64KB
const SNAPSHOT_CHUNK_SIZE: u64 = 64 * 1024;

/// Connect interface between server and clients
#[allow(clippy::module_name_repetitions)] // better for recognizing than just "Api"
#[cfg_attr(test, automock)]
#[async_trait]
pub(crate) trait ConnectApi: Send + Sync + 'static {
    /// Get server id
    fn id(&self) -> ServerId;

    /// Update server addresses, the new addresses will override the old ones
    async fn update_addrs(&self, addrs: Vec<String>) -> Result<(), CurpError>;

    /// Send `ProposeRequest`
    async fn propose_stream(
        &self,
        request: ProposeRequest,
        token: Option<String>,
        timeout: Duration,
    ) -> Result<Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send>, CurpError>;

    /// Send `RecordRequest`
    async fn record(
        &self,
        request: RecordRequest,
        timeout: Duration,
    ) -> Result<RecordResponse, CurpError>;

    /// Send `ReadIndexRequest`
    async fn read_index(&self, timeout: Duration) -> Result<ReadIndexResponse, CurpError>;

    /// Send `ProposeRequest`
    async fn propose_conf_change(
        &self,
        request: ProposeConfChangeRequest,
        timeout: Duration,
    ) -> Result<ProposeConfChangeResponse, CurpError>;

    /// Send `PublishRequest`
    async fn publish(
        &self,
        request: PublishRequest,
        timeout: Duration,
    ) -> Result<PublishResponse, CurpError>;

    /// Send `ShutdownRequest`
    async fn shutdown(
        &self,
        request: ShutdownRequest,
        timeout: Duration,
    ) -> Result<ShutdownResponse, CurpError>;

    /// Send `FetchClusterRequest`
    async fn fetch_cluster(
        &self,
        request: FetchClusterRequest,
        timeout: Duration,
    ) -> Result<FetchClusterResponse, CurpError>;

    /// Send `FetchReadStateRequest`
    async fn fetch_read_state(
        &self,
        request: FetchReadStateRequest,
        timeout: Duration,
    ) -> Result<FetchReadStateResponse, CurpError>;

    /// Send `MoveLeaderRequest`
    async fn move_leader(
        &self,
        request: MoveLeaderRequest,
        timeout: Duration,
    ) -> Result<MoveLeaderResponse, CurpError>;

    /// Keep send lease keep alive to server and mutate the client id
    async fn lease_keep_alive(&self, client_id: Arc<AtomicU64>, interval: Duration) -> CurpError;
}

/// Inner Connect interface among different servers
#[cfg_attr(test, automock)]
#[async_trait]
pub(crate) trait InnerConnectApi: Send + Sync + 'static {
    /// Get server id
    fn id(&self) -> ServerId;

    /// Update server addresses, the new addresses will override the old ones
    async fn update_addrs(&self, addrs: Vec<String>) -> Result<(), CurpError>;

    /// Send `AppendEntriesRequest`
    async fn append_entries(
        &self,
        request: AppendEntriesRequest,
        timeout: Duration,
    ) -> Result<AppendEntriesResponse, CurpError>;

    /// Send `VoteRequest`
    async fn vote(
        &self,
        request: VoteRequest,
        timeout: Duration,
    ) -> Result<VoteResponse, CurpError>;

    /// Send a snapshot
    async fn install_snapshot(
        &self,
        term: u64,
        leader_id: ServerId,
        snapshot: Snapshot,
    ) -> Result<InstallSnapshotResponse, CurpError>;

    /// Trigger follower shutdown
    async fn trigger_shutdown(&self) -> Result<(), CurpError>;

    /// Send `TryBecomeLeaderNowRequest`
    async fn try_become_leader_now(&self, timeout: Duration) -> Result<(), CurpError>;
}

/// Inner Connect Api Wrapper
/// The solution of [rustc bug](https://github.com/dtolnay/async-trait/issues/141#issuecomment-767978616)
#[derive(Clone)]
pub(crate) struct InnerConnectApiWrapper(Arc<dyn InnerConnectApi>);

impl InnerConnectApiWrapper {
    /// Create a new `InnerConnectApiWrapper` from `Arc<dyn ConnectApi>`
    pub(crate) fn new_from_arc(connect: Arc<dyn InnerConnectApi>) -> Self {
        Self(connect)
    }
}

impl Debug for InnerConnectApiWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InnerConnectApiWrapper").finish()
    }
}

impl Deref for InnerConnectApiWrapper {
    type Target = Arc<dyn InnerConnectApi>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A connect api implementation which bypass kernel to dispatch method directly.
pub(crate) struct BypassedConnect<T: super::CurpService> {
    /// inner server
    server: T,
    /// server id
    id: ServerId,
}

impl<T: super::CurpService> BypassedConnect<T> {
    /// Create a bypassed connect
    pub(crate) fn new(id: ServerId, server: T) -> Self {
        Self { server, id }
    }
}

/// Metadata key of a bypassed request
const BYPASS_KEY: &str = "bypass";

/// Build a `Metadata` with bypass flag and tracing context injected
fn bypassed_metadata() -> super::Metadata {
    let mut meta = super::Metadata::new();
    meta.insert(BYPASS_KEY, "true");
    meta.inject_current();
    meta
}

/// Build a `Metadata` with bypass flag, tracing context, and optional token
fn bypassed_metadata_with_token(token: Option<String>) -> super::Metadata {
    let mut meta = bypassed_metadata();
    if let Some(token) = token {
        meta.insert("token", token);
    }
    meta
}

#[async_trait]
impl<T> ConnectApi for BypassedConnect<T>
where
    T: super::CurpService,
{
    /// Get server id
    fn id(&self) -> ServerId {
        self.id
    }

    /// Update server addresses, the new addresses will override the old ones
    async fn update_addrs(&self, _addrs: Vec<String>) -> Result<(), CurpError> {
        // bypassed connect never updates its addresses
        Ok(())
    }

    /// Send `ProposeRequest`
    #[instrument(skip(self), name = "client propose stream")]
    async fn propose_stream(
        &self,
        request: ProposeRequest,
        token: Option<String>,
        _timeout: Duration,
    ) -> Result<Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send>, CurpError> {
        let meta = bypassed_metadata_with_token(token);
        let stream = self.server.propose_stream(request, meta).await?;
        Ok(Box::new(stream))
    }

    /// Send `RecordRequest`
    #[instrument(skip(self), name = "client record")]
    async fn record(
        &self,
        request: RecordRequest,
        _timeout: Duration,
    ) -> Result<RecordResponse, CurpError> {
        let meta = bypassed_metadata();
        self.server.record(request, meta)
    }

    async fn read_index(&self, _timeout: Duration) -> Result<ReadIndexResponse, CurpError> {
        let meta = bypassed_metadata();
        self.server.read_index(meta)
    }

    /// Send `PublishRequest`
    async fn publish(
        &self,
        request: PublishRequest,
        _timeout: Duration,
    ) -> Result<PublishResponse, CurpError> {
        let meta = bypassed_metadata();
        self.server.publish(request, meta)
    }

    /// Send `ProposeRequest`
    async fn propose_conf_change(
        &self,
        request: ProposeConfChangeRequest,
        _timeout: Duration,
    ) -> Result<ProposeConfChangeResponse, CurpError> {
        let meta = bypassed_metadata();
        self.server.propose_conf_change(request, meta).await
    }

    /// Send `ShutdownRequest`
    async fn shutdown(
        &self,
        request: ShutdownRequest,
        _timeout: Duration,
    ) -> Result<ShutdownResponse, CurpError> {
        let meta = bypassed_metadata();
        self.server.shutdown(request, meta).await
    }

    /// Send `FetchClusterRequest`
    async fn fetch_cluster(
        &self,
        request: FetchClusterRequest,
        _timeout: Duration,
    ) -> Result<FetchClusterResponse, CurpError> {
        self.server.fetch_cluster(request)
    }

    /// Send `FetchReadStateRequest`
    async fn fetch_read_state(
        &self,
        request: FetchReadStateRequest,
        _timeout: Duration,
    ) -> Result<FetchReadStateResponse, CurpError> {
        self.server.fetch_read_state(request)
    }

    /// Send `MoveLeaderRequest`
    async fn move_leader(
        &self,
        request: MoveLeaderRequest,
        _timeout: Duration,
    ) -> Result<MoveLeaderResponse, CurpError> {
        self.server.move_leader(request).await
    }

    /// Keep send lease keep alive to server and mutate the client id
    async fn lease_keep_alive(&self, _client_id: Arc<AtomicU64>, _interval: Duration) -> CurpError {
        unreachable!("cannot invoke lease_keep_alive in bypassed connect")
    }
}

/// Generate install snapshot stream
fn install_snapshot_stream(
    term: u64,
    leader_id: ServerId,
    snapshot: Snapshot,
) -> impl Stream<Item = InstallSnapshotRequest> {
    stream! {
        let meta = snapshot.meta;
        let mut snapshot = snapshot.into_inner();
        let mut offset = 0;
        if let Err(e) = snapshot.rewind() {
            error!("snapshot seek failed, {e}");
            return;
        }
        #[allow(clippy::arithmetic_side_effects)] // can't overflow
        while offset < snapshot.size() {
            let len: u64 =
                std::cmp::min(snapshot.size() - offset, SNAPSHOT_CHUNK_SIZE).numeric_cast();
            let mut data = BytesMut::with_capacity(len.numeric_cast());
            if let Err(e) = snapshot.read_buf_exact(&mut data).await {
                error!("read snapshot error, {e}");
                break;
            }
            yield InstallSnapshotRequest {
                term,
                leader_id,
                last_included_index: meta.last_included_index,
                last_included_term: meta.last_included_term,
                offset,
                data: data.freeze(),
                done: (offset + len) == snapshot.size(),
            };

            offset += len;
        }
        // TODO: Shall we clean snapshot after stream generation complete
        if let Err(e) = snapshot.clean().await {
            error!("snapshot clean error, {e}");
        }
    }
}

// ============================================================================
// QUIC transport implementations
// ============================================================================

mod quic_connect_impl {
    use std::{
        sync::{Arc, atomic::AtomicU64},
        time::Duration,
    };

    use async_trait::async_trait;
    use futures::Stream;
    use gm_quic::prelude::QuicClient;

    use crate::{
        members::ServerId,
        rpc::{
            AppendEntriesRequest, AppendEntriesResponse, CurpError, FetchClusterRequest,
            FetchClusterResponse, FetchReadStateRequest, FetchReadStateResponse,
            InstallSnapshotResponse, LeaseKeepAliveMsg, MethodId, MoveLeaderRequest,
            MoveLeaderResponse, OpResponse, ProposeConfChangeRequest, ProposeConfChangeResponse,
            ProposeRequest, PublishRequest, PublishResponse, ReadIndexResponse, RecordRequest,
            RecordResponse, ShutdownRequest, ShutdownResponse, VoteRequest, VoteResponse,
            quic_transport::channel::QuicChannel,
        },
        snapshot::Snapshot,
    };

    use super::{ConnectApi, InnerConnectApi, InnerConnectApiWrapper};

    /// QUIC implementation of `ConnectApi`
    pub(crate) struct QuicConnect {
        /// Server id
        id: ServerId,
        /// QUIC channel
        channel: Arc<QuicChannel>,
    }

    impl std::fmt::Debug for QuicConnect {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("QuicConnect").field("id", &self.id).finish()
        }
    }

    impl QuicConnect {
        /// Create a new QUIC connect
        pub(crate) fn new(id: ServerId, channel: Arc<QuicChannel>) -> Self {
            Self { id, channel }
        }
    }

    #[async_trait]
    impl ConnectApi for QuicConnect {
        fn id(&self) -> ServerId {
            self.id
        }

        async fn update_addrs(&self, addrs: Vec<String>) -> Result<(), CurpError> {
            self.channel.update_addrs(addrs).await
        }

        async fn propose_stream(
            &self,
            request: ProposeRequest,
            token: Option<String>,
            timeout: Duration,
        ) -> Result<Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send>, CurpError>
        {
            let mut meta = Vec::new();
            if let Some(token) = token {
                meta.push(("token".to_owned(), token));
            }
            self.channel
                .server_streaming_call(MethodId::ProposeStream, request, meta, timeout)
                .await
                .map(
                    |s| -> Box<dyn Stream<Item = Result<OpResponse, CurpError>> + Send> {
                        Box::new(s)
                    },
                )
        }

        async fn record(
            &self,
            request: RecordRequest,
            timeout: Duration,
        ) -> Result<RecordResponse, CurpError> {
            self.channel
                .unary_call(MethodId::Record, request, vec![], timeout)
                .await
        }

        async fn read_index(&self, timeout: Duration) -> Result<ReadIndexResponse, CurpError> {
            use crate::rpc::proto::commandpb::ReadIndexRequest;
            self.channel
                .unary_call(MethodId::ReadIndex, ReadIndexRequest {}, vec![], timeout)
                .await
        }

        async fn propose_conf_change(
            &self,
            request: ProposeConfChangeRequest,
            timeout: Duration,
        ) -> Result<ProposeConfChangeResponse, CurpError> {
            self.channel
                .unary_call(MethodId::ProposeConfChange, request, vec![], timeout)
                .await
        }

        async fn publish(
            &self,
            request: PublishRequest,
            timeout: Duration,
        ) -> Result<PublishResponse, CurpError> {
            self.channel
                .unary_call(MethodId::Publish, request, vec![], timeout)
                .await
        }

        async fn shutdown(
            &self,
            request: ShutdownRequest,
            timeout: Duration,
        ) -> Result<ShutdownResponse, CurpError> {
            self.channel
                .unary_call(MethodId::Shutdown, request, vec![], timeout)
                .await
        }

        async fn fetch_cluster(
            &self,
            request: FetchClusterRequest,
            timeout: Duration,
        ) -> Result<FetchClusterResponse, CurpError> {
            self.channel
                .unary_call(MethodId::FetchCluster, request, vec![], timeout)
                .await
        }

        async fn fetch_read_state(
            &self,
            request: FetchReadStateRequest,
            timeout: Duration,
        ) -> Result<FetchReadStateResponse, CurpError> {
            self.channel
                .unary_call(MethodId::FetchReadState, request, vec![], timeout)
                .await
        }

        async fn move_leader(
            &self,
            request: MoveLeaderRequest,
            timeout: Duration,
        ) -> Result<MoveLeaderResponse, CurpError> {
            self.channel
                .unary_call(MethodId::MoveLeader, request, vec![], timeout)
                .await
        }

        async fn lease_keep_alive(
            &self,
            client_id: Arc<AtomicU64>,
            interval: Duration,
        ) -> CurpError {
            use async_stream::stream;
            use tracing::{debug, info};

            loop {
                let cid = client_id.load(std::sync::atomic::Ordering::Relaxed);
                let stream = stream! {
                    let mut ticker = tokio::time::interval(interval);
                    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    loop {
                        _ = ticker.tick().await;
                        if cid == 0 {
                            debug!("client request a client id");
                        } else {
                            debug!("client keep alive the client id({cid})");
                        }
                        yield LeaseKeepAliveMsg { client_id: cid };
                    }
                };
                let result: Result<LeaseKeepAliveMsg, CurpError> = self
                    .channel
                    .client_streaming_call(
                        MethodId::LeaseKeepAlive,
                        Box::pin(stream),
                        vec![],
                        Duration::from_secs(30),
                    )
                    .await;
                match result {
                    Err(e) => return e,
                    Ok(msg) => {
                        info!("client_id update to {}", msg.client_id);
                        client_id.store(msg.client_id, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }
    }

    /// QUIC implementation of `InnerConnectApi`
    pub(crate) struct QuicInnerConnect {
        /// Server id
        id: ServerId,
        /// QUIC channel
        channel: Arc<QuicChannel>,
    }

    impl std::fmt::Debug for QuicInnerConnect {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("QuicInnerConnect")
                .field("id", &self.id)
                .finish()
        }
    }

    impl QuicInnerConnect {
        /// Create a new QUIC inner connect
        pub(crate) fn new(id: ServerId, channel: Arc<QuicChannel>) -> Self {
            Self { id, channel }
        }
    }

    #[async_trait]
    impl InnerConnectApi for QuicInnerConnect {
        fn id(&self) -> ServerId {
            self.id
        }

        async fn update_addrs(&self, addrs: Vec<String>) -> Result<(), CurpError> {
            self.channel.update_addrs(addrs).await
        }

        async fn append_entries(
            &self,
            request: AppendEntriesRequest,
            timeout: Duration,
        ) -> Result<AppendEntriesResponse, CurpError> {
            self.channel
                .unary_call(MethodId::AppendEntries, request, vec![], timeout)
                .await
        }

        async fn vote(
            &self,
            request: VoteRequest,
            timeout: Duration,
        ) -> Result<VoteResponse, CurpError> {
            self.channel
                .unary_call(MethodId::Vote, request, vec![], timeout)
                .await
        }

        async fn install_snapshot(
            &self,
            term: u64,
            leader_id: ServerId,
            snapshot: Snapshot,
        ) -> Result<InstallSnapshotResponse, CurpError> {
            let stream = super::install_snapshot_stream(term, leader_id, snapshot);
            self.channel
                .client_streaming_call(
                    MethodId::InstallSnapshot,
                    Box::pin(stream),
                    vec![],
                    Duration::from_secs(60),
                )
                .await
        }

        async fn trigger_shutdown(&self) -> Result<(), CurpError> {
            use crate::rpc::TriggerShutdownRequest;
            use crate::rpc::proto::inner_messagepb::TriggerShutdownResponse;
            let _resp: TriggerShutdownResponse = self
                .channel
                .unary_call(
                    MethodId::TriggerShutdown,
                    TriggerShutdownRequest::default(),
                    vec![],
                    Duration::from_secs(5),
                )
                .await?;
            Ok(())
        }

        async fn try_become_leader_now(&self, timeout: Duration) -> Result<(), CurpError> {
            use crate::rpc::TryBecomeLeaderNowRequest;
            use crate::rpc::proto::inner_messagepb::TryBecomeLeaderNowResponse;
            let _resp: TryBecomeLeaderNowResponse = self
                .channel
                .unary_call(
                    MethodId::TryBecomeLeaderNow,
                    TryBecomeLeaderNowRequest::default(),
                    vec![],
                    timeout,
                )
                .await?;
            Ok(())
        }
    }

    // ========================================================================
    // QUIC factory functions (now the primary connect/connects/inner_connects)
    // ========================================================================

    use crate::rpc::quic_transport::channel::DnsFallback;

    /// Create a single QUIC connect
    pub(crate) fn quic_connect(
        id: ServerId,
        addrs: Vec<String>,
        client: &Arc<QuicClient>,
        dns_fallback: DnsFallback,
    ) -> Arc<dyn ConnectApi> {
        let channel = Arc::new(QuicChannel::with_addrs(
            Arc::clone(client),
            addrs,
            dns_fallback,
        ));
        Arc::new(QuicConnect::new(id, channel))
    }

    /// Create QUIC connects for all members
    pub(crate) fn quic_connects(
        members: std::collections::HashMap<ServerId, Vec<String>>,
        client: &Arc<QuicClient>,
        dns_fallback: DnsFallback,
    ) -> impl Iterator<Item = (ServerId, Arc<dyn ConnectApi>)> + use<> {
        let client = Arc::clone(client);
        members
            .into_iter()
            .map(move |(id, addrs)| (id, quic_connect(id, addrs, &client, dns_fallback)))
    }

    /// Create QUIC inner connects for all members
    pub(crate) fn quic_inner_connects(
        members: std::collections::HashMap<ServerId, Vec<String>>,
        client: &Arc<QuicClient>,
        dns_fallback: DnsFallback,
    ) -> impl Iterator<Item = (ServerId, InnerConnectApiWrapper)> + use<> {
        let client = Arc::clone(client);
        members.into_iter().map(move |(id, addrs)| {
            let channel = Arc::new(QuicChannel::with_addrs(
                Arc::clone(&client),
                addrs,
                dns_fallback,
            ));
            (
                id,
                InnerConnectApiWrapper::new_from_arc(Arc::new(QuicInnerConnect::new(id, channel))),
            )
        })
    }
}

#[allow(unused_imports)]
pub(crate) use quic_connect_impl::{
    QuicConnect, QuicInnerConnect, quic_connect, quic_connects, quic_inner_connects,
};

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use engine::{EngineType, Snapshot as EngineSnapshot};
    use futures::{StreamExt, pin_mut};
    use test_macros::abort_on_panic;
    use tracing_test::traced_test;

    use super::*;
    use crate::snapshot::SnapshotMeta;

    #[traced_test]
    #[tokio::test]
    #[abort_on_panic]
    async fn test_install_snapshot_stream() {
        const SNAPSHOT_SIZE: u64 = 200 * 1024;
        let mut snapshot = EngineSnapshot::new_for_receiving(EngineType::Memory).unwrap();
        snapshot
            .write_all(Bytes::from(vec![1; SNAPSHOT_SIZE.numeric_cast()]))
            .await
            .unwrap();
        let stream = install_snapshot_stream(
            0,
            123,
            Snapshot::new(
                SnapshotMeta {
                    last_included_index: 1,
                    last_included_term: 1,
                },
                snapshot,
            ),
        );
        pin_mut!(stream);
        let mut sum = 0;
        while let Some(req) = stream.next().await {
            assert_eq!(req.term, 0);
            assert_eq!(req.leader_id, 123);
            assert_eq!(req.last_included_index, 1);
            assert_eq!(req.last_included_term, 1);
            sum += req.data.len() as u64;
            assert_eq!(sum == SNAPSHOT_SIZE, req.done);
        }
        assert_eq!(sum, SNAPSHOT_SIZE);
    }
}
