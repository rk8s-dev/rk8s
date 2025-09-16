//! Etcd Maintenance RPC.

pub use crate::rpc::pb::etcdserverpb::alarm_request::AlarmAction;
pub use crate::rpc::pb::etcdserverpb::AlarmType;

use super::pb::etcdserverpb;
use crate::auth::AuthService;
use crate::error::Result;
use crate::intercept::InterceptedChannel;
use crate::rpc::pb::etcdserverpb::{
    AlarmRequest as PbAlarmRequest, AlarmResponse as PbAlarmResponse,
    DefragmentRequest as PbDefragmentRequest, DefragmentResponse as PbDefragmentResponse,
    HashKvRequest as PbHashKvRequest, HashKvResponse as PbHashKvResponse,
    HashRequest as PbHashRequest, HashResponse as PbHashResponse,
    MoveLeaderRequest as PbMoveLeaderRequest, MoveLeaderResponse as PbMoveLeaderResponse,
    SnapshotRequest as PbSnapshotRequest, SnapshotResponse as PbSnapshotResponse,
    StatusRequest as PbStatusRequest, StatusResponse as PbStatusResponse,
};
use crate::rpc::ResponseHeader;
use etcdserverpb::maintenance_client::MaintenanceClient as PbMaintenanceClient;
use etcdserverpb::AlarmMember as PbAlarmMember;
use http::HeaderValue;
use std::sync::{Arc, RwLock};
use tonic::codec::Streaming as PbStreaming;
use tonic::{IntoRequest, Request};

/// Client for maintenance operations.
#[repr(transparent)]
#[derive(Clone)]
pub struct MaintenanceClient {
    inner: PbMaintenanceClient<AuthService<InterceptedChannel>>,
}

/// Options for `alarm` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct AlarmOptions(PbAlarmRequest);

impl AlarmOptions {
    /// Creates a new `AlarmOptions`.
    #[inline]
    pub const fn new() -> Self {
        AlarmOptions(PbAlarmRequest {
            action: AlarmAction::Get as i32,
            member_id: 0,
            alarm: AlarmType::None as i32,
        })
    }

    /// Sets alarm action and alarm type.
    #[inline]
    const fn with_action_and_type(
        mut self,
        alarm_action: AlarmAction,
        alarm_type: AlarmType,
    ) -> Self {
        self.0.action = alarm_action as i32;
        self.0.alarm = alarm_type as i32;
        self
    }

    /// Sets alarm member.
    #[inline]
    pub fn with_member(&mut self, member: u64) {
        self.0.member_id = member;
    }
}

impl From<AlarmOptions> for PbAlarmRequest {
    #[inline]
    fn from(alarm: AlarmOptions) -> Self {
        alarm.0
    }
}

impl IntoRequest<PbAlarmRequest> for AlarmOptions {
    #[inline]
    fn into_request(self) -> Request<PbAlarmRequest> {
        Request::new(self.into())
    }
}

/// Options for `status` operation.
#[derive(Debug, Default, Clone)]
struct StatusOptions(PbStatusRequest);

impl StatusOptions {
    #[inline]
    const fn new() -> Self {
        Self(PbStatusRequest {})
    }
}

impl From<StatusOptions> for PbStatusRequest {
    #[inline]
    fn from(status: StatusOptions) -> Self {
        status.0
    }
}

impl IntoRequest<PbStatusRequest> for StatusOptions {
    #[inline]
    fn into_request(self) -> Request<PbStatusRequest> {
        Request::new(self.into())
    }
}

/// Options for `defragment` operation.
#[derive(Debug, Default, Clone)]
struct DefragmentOptions(PbDefragmentRequest);

impl DefragmentOptions {
    #[inline]
    const fn new() -> Self {
        Self(PbDefragmentRequest {})
    }
}

impl From<DefragmentOptions> for PbDefragmentRequest {
    #[inline]
    fn from(defragment: DefragmentOptions) -> Self {
        defragment.0
    }
}

impl IntoRequest<PbDefragmentRequest> for DefragmentOptions {
    #[inline]
    fn into_request(self) -> Request<PbDefragmentRequest> {
        Request::new(self.into())
    }
}

/// Options for `hash` operation.
#[derive(Debug, Default, Clone)]
struct HashOptions(PbHashRequest);

impl HashOptions {
    #[inline]
    const fn new() -> Self {
        Self(PbHashRequest {})
    }
}

impl From<HashOptions> for PbHashRequest {
    #[inline]
    fn from(hash: HashOptions) -> Self {
        hash.0
    }
}

impl IntoRequest<PbHashRequest> for HashOptions {
    #[inline]
    fn into_request(self) -> Request<PbHashRequest> {
        Request::new(self.into())
    }
}

/// Options for `hashkv` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
struct HashKvOptions(PbHashKvRequest);

impl HashKvOptions {
    #[inline]
    const fn new(revision: i64) -> Self {
        Self(PbHashKvRequest { revision })
    }
}

impl From<HashKvOptions> for PbHashKvRequest {
    #[inline]
    fn from(hash_kv: HashKvOptions) -> Self {
        hash_kv.0
    }
}

impl IntoRequest<PbHashKvRequest> for HashKvOptions {
    #[inline]
    fn into_request(self) -> Request<PbHashKvRequest> {
        Request::new(self.into())
    }
}

/// Options for `snapshot` operation.
#[derive(Debug, Default, Clone)]
struct SnapshotOptions(PbSnapshotRequest);

impl SnapshotOptions {
    #[inline]
    const fn new() -> Self {
        Self(PbSnapshotRequest {})
    }
}

impl From<SnapshotOptions> for PbSnapshotRequest {
    #[inline]
    fn from(snapshot: SnapshotOptions) -> Self {
        snapshot.0
    }
}

impl IntoRequest<PbSnapshotRequest> for SnapshotOptions {
    #[inline]
    fn into_request(self) -> Request<PbSnapshotRequest> {
        Request::new(self.into())
    }
}

/// Response for `alarm` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct AlarmResponse(PbAlarmResponse);

/// Alarm member of respond.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Clone, PartialEq, Eq)]
pub struct AlarmMember {
    /// memberID is the ID of the member associated with the raised alarm.
    member_id: u64,
    /// alarm is the type of alarm which has been raised.
    alarm: AlarmType,
}

impl AlarmMember {
    /// Get member id.
    #[inline]
    pub fn member_id(&self) -> u64 {
        self.member_id
    }

    /// Get alarm.
    #[inline]
    pub fn alarm(&self) -> AlarmType {
        self.alarm
    }
}

impl AlarmResponse {
    /// Create a new `AlarmResponse` from pb put response.
    #[inline]
    const fn new(resp: PbAlarmResponse) -> Self {
        Self(resp)
    }

    /// Get response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// Get alarms of members.
    #[inline]
    pub fn alarms(&self) -> &[AlarmMember] {
        unsafe { &*(&self.0.alarms as *const Vec<PbAlarmMember> as *const Vec<AlarmMember>) }
    }
}

/// Response for `status` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct StatusResponse(PbStatusResponse);

impl StatusResponse {
    /// Create a new `StatusResponse` from pb put response.
    #[inline]
    const fn new(resp: PbStatusResponse) -> Self {
        Self(resp)
    }

    /// Get response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// Get version of the member.
    #[inline]
    pub fn version(&self) -> &str {
        &self.0.version
    }

    /// Get size of db, in bytes.
    #[inline]
    pub fn db_size(&self) -> i64 {
        self.0.db_size
    }

    /// Get leader of cluster.
    #[inline]
    pub fn leader(&self) -> u64 {
        self.0.leader
    }

    /// Get raft index of cluster.
    #[inline]
    pub fn raft_index(&self) -> u64 {
        self.0.raft_index
    }

    /// Get raft term of cluster.
    #[inline]
    pub fn raft_term(&self) -> u64 {
        self.0.raft_term
    }

    /// Get raft applied of respond member.
    #[inline]
    pub fn raft_applied_index(&self) -> u64 {
        self.0.raft_applied_index
    }

    /// Get errors of cluster members.
    #[inline]
    pub fn errors(&self) -> &[String] {
        &self.0.errors
    }

    /// Get raft used db size, in bytes.
    #[inline]
    pub fn raft_used_db_size(&self) -> i64 {
        self.0.db_size_in_use
    }

    /// Indicate if the member is raft learner.
    #[inline]
    pub fn is_learner(&self) -> bool {
        self.0.is_learner
    }
}

/// Response for `defragment` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct DefragmentResponse(PbDefragmentResponse);

impl DefragmentResponse {
    /// Create a new `DefragmentResponse` from pb put response.
    #[inline]
    const fn new(resp: PbDefragmentResponse) -> Self {
        Self(resp)
    }

    /// Get response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Response for `hash` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct HashResponse(PbHashResponse);

impl HashResponse {
    /// Create a new `HashResponse` from pb put response.
    #[inline]
    const fn new(resp: PbHashResponse) -> Self {
        Self(resp)
    }

    /// Get response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// Gets the hash value computed from the responding member's KV's backend.
    #[inline]
    pub fn hash(&self) -> u32 {
        self.0.hash
    }
}

/// Response for `hash_kv` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct HashKvResponse(PbHashKvResponse);

impl HashKvResponse {
    /// Create a new `HashKvResponse` from pb put response.
    #[inline]
    const fn new(resp: PbHashKvResponse) -> Self {
        Self(resp)
    }

    /// Get response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// Gets the hash value computed from the responding member's MVCC keys up to a given revision.
    #[inline]
    pub fn hash(&self) -> u32 {
        self.0.hash
    }

    /// Gets compacted revision of key-value store when hash begins.
    #[inline]
    pub fn compact_version(&self) -> i64 {
        self.0.compact_revision
    }
}

/// Response for `snapshot` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct SnapshotResponse(PbSnapshotResponse);

impl SnapshotResponse {
    /// Create a new `SnapshotResponse` from pb put response.
    #[inline]
    const fn new(resp: PbSnapshotResponse) -> Self {
        Self(resp)
    }

    /// Get response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// Get remaining bytes.
    #[inline]
    pub fn remaining_bytes(&self) -> u64 {
        self.0.remaining_bytes
    }

    /// The next chunk of the snapshot in the snapshot stream.
    #[inline]
    pub fn blob(&self) -> &[u8] {
        &self.0.blob
    }
}

/// Response for `snapshot` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug)]
#[repr(transparent)]
pub struct SnapshotStreaming(PbStreaming<PbSnapshotResponse>);

impl SnapshotStreaming {
    /// Fetches the next message from this stream.
    #[inline]
    pub async fn message(&mut self) -> Result<Option<SnapshotResponse>> {
        let ret = self.0.message().await?;
        match ret {
            Some(rsp) => Ok(Some(SnapshotResponse::new(rsp))),
            None => Ok(None),
        }
    }
}

/// Options for `MoveLeader` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct MoveLeaderOptions(PbMoveLeaderRequest);

impl MoveLeaderOptions {
    /// Sets target_id
    #[inline]
    const fn with_target_id(mut self, target_id: u64) -> Self {
        self.0.target_id = target_id;
        self
    }

    /// Creates a `MoveLeaderOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbMoveLeaderRequest { target_id: 0 })
    }
}

impl From<MoveLeaderOptions> for PbMoveLeaderRequest {
    #[inline]
    fn from(options: MoveLeaderOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbMoveLeaderRequest> for MoveLeaderOptions {
    #[inline]
    fn into_request(self) -> Request<PbMoveLeaderRequest> {
        Request::new(self.into())
    }
}

/// Response for `MoveLeader` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct MoveLeaderResponse(PbMoveLeaderResponse);

impl MoveLeaderResponse {
    #[inline]
    const fn new(resp: PbMoveLeaderResponse) -> Self {
        Self(resp)
    }

    /// Get response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

impl MaintenanceClient {
    /// Creates a maintenance client.
    #[inline]
    pub(crate) fn new(
        channel: InterceptedChannel,
        auth_token: Arc<RwLock<Option<HeaderValue>>>,
    ) -> Self {
        let inner = PbMaintenanceClient::new(AuthService::new(channel, auth_token));
        Self { inner }
    }

    /// Get or active or inactive alarm.
    #[inline]
    pub async fn alarm(
        &mut self,
        alarm_action: AlarmAction,
        alarm_type: AlarmType,
        options: Option<AlarmOptions>,
    ) -> Result<AlarmResponse> {
        let resp = self
            .inner
            .alarm(
                options
                    .unwrap_or_default()
                    .with_action_and_type(alarm_action, alarm_type),
            )
            .await?
            .into_inner();
        Ok(AlarmResponse::new(resp))
    }

    /// Get status of a member.
    #[inline]
    pub async fn status(&mut self) -> Result<StatusResponse> {
        let resp = self.inner.status(StatusOptions::new()).await?.into_inner();
        Ok(StatusResponse::new(resp))
    }

    /// Defragment a member's backend database to recover storage space.
    #[inline]
    pub async fn defragment(&mut self) -> Result<DefragmentResponse> {
        let resp = self
            .inner
            .defragment(DefragmentOptions::new())
            .await?
            .into_inner();
        Ok(DefragmentResponse::new(resp))
    }

    /// Computes the hash of whole backend keyspace.
    /// including key, lease, and other buckets in storage.
    /// This is designed for testing ONLY!
    #[inline]
    pub async fn hash(&mut self) -> Result<HashResponse> {
        let resp = self.inner.hash(HashOptions::new()).await?.into_inner();
        Ok(HashResponse::new(resp))
    }

    /// Computes the hash of all MVCC keys up to a given revision.
    /// It only iterates \"key\" bucket in backend storage.
    #[inline]
    pub async fn hash_kv(&mut self, revision: i64) -> Result<HashKvResponse> {
        let resp = self
            .inner
            .hash_kv(HashKvOptions::new(revision))
            .await?
            .into_inner();
        Ok(HashKvResponse::new(resp))
    }

    /// Gets a snapshot of the entire backend from a member over a stream to a client.
    #[inline]
    pub async fn snapshot(&mut self) -> Result<SnapshotStreaming> {
        let resp = self
            .inner
            .snapshot(SnapshotOptions::new())
            .await?
            .into_inner();
        Ok(SnapshotStreaming(resp))
    }

    /// Moves the current leader node to target node.
    #[inline]
    pub async fn move_leader(&mut self, target_id: u64) -> Result<MoveLeaderResponse> {
        let resp = self
            .inner
            .move_leader(MoveLeaderOptions::new().with_target_id(target_id))
            .await?
            .into_inner();
        Ok(MoveLeaderResponse::new(resp))
    }
}
