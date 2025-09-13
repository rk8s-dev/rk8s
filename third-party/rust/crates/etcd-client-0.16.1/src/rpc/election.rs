//! Etcd Election RPC.

use crate::auth::AuthService;
use crate::error::Result;
use crate::intercept::InterceptedChannel;
use crate::rpc::pb::v3electionpb::election_client::ElectionClient as PbElectionClient;
use crate::rpc::pb::v3electionpb::{
    CampaignRequest as PbCampaignRequest, CampaignResponse as PbCampaignResponse,
    LeaderKey as PbLeaderKey, LeaderRequest as PbLeaderRequest, LeaderResponse as PbLeaderResponse,
    ProclaimRequest as PbProclaimRequest, ProclaimResponse as PbProclaimResponse,
    ResignRequest as PbResignRequest, ResignResponse as PbResignResponse,
};
use crate::rpc::{KeyValue, ResponseHeader};
use http::HeaderValue;
use std::sync::RwLock;
use std::task::{Context, Poll};
use std::{pin::Pin, sync::Arc};
use tokio_stream::Stream;
use tonic::{IntoRequest, Request, Streaming};

/// Client for Elect operations.
#[repr(transparent)]
#[derive(Clone)]
pub struct ElectionClient {
    inner: PbElectionClient<AuthService<InterceptedChannel>>,
}

/// Options for `campaign` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct CampaignOptions(PbCampaignRequest);

impl CampaignOptions {
    #[inline]
    pub const fn new() -> Self {
        Self(PbCampaignRequest {
            name: Vec::new(),
            lease: 0,
            value: Vec::new(),
        })
    }

    /// Name is the election's identifier for the campaign.
    #[inline]
    fn with_name(mut self, name: impl Into<Vec<u8>>) -> Self {
        self.0.name = name.into();
        self
    }

    /// Lease is the ID of the lease attached to leadership of the election
    #[inline]
    const fn with_lease(mut self, lease: i64) -> Self {
        self.0.lease = lease;
        self
    }

    /// Value is the initial proclaimed value set when the campaigner wins the election.
    #[inline]
    fn with_value(mut self, value: impl Into<Vec<u8>>) -> Self {
        self.0.value = value.into();
        self
    }
}

impl From<CampaignOptions> for PbCampaignRequest {
    #[inline]
    fn from(options: CampaignOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbCampaignRequest> for CampaignOptions {
    #[inline]
    fn into_request(self) -> Request<PbCampaignRequest> {
        Request::new(self.into())
    }
}

/// Options for `proclaim` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct ProclaimOptions(PbProclaimRequest);

impl ProclaimOptions {
    #[inline]
    pub const fn new() -> Self {
        Self(PbProclaimRequest {
            leader: None,
            value: Vec::new(),
        })
    }

    /// The initial proclaimed value set when the campaigner wins the election.
    #[inline]
    fn with_value(mut self, value: impl Into<Vec<u8>>) -> Self {
        self.0.value = value.into();
        self
    }

    /// The leadership hold on the election.
    #[inline]
    pub fn with_leader(mut self, leader: LeaderKey) -> Self {
        self.0.leader = Some(leader.into());
        self
    }
}

impl From<ProclaimOptions> for PbProclaimRequest {
    #[inline]
    fn from(options: ProclaimOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbProclaimRequest> for ProclaimOptions {
    #[inline]
    fn into_request(self) -> Request<PbProclaimRequest> {
        Request::new(self.into())
    }
}

/// Options for `leader` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct LeaderOptions(PbLeaderRequest);

impl LeaderOptions {
    #[inline]
    pub const fn new() -> Self {
        Self(PbLeaderRequest { name: Vec::new() })
    }

    /// Name is the election identifier for the leadership information.
    #[inline]
    pub fn with_name(mut self, name: impl Into<Vec<u8>>) -> Self {
        self.0.name = name.into();
        self
    }
}

impl From<LeaderOptions> for PbLeaderRequest {
    #[inline]
    fn from(options: LeaderOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbLeaderRequest> for LeaderOptions {
    #[inline]
    fn into_request(self) -> Request<PbLeaderRequest> {
        Request::new(self.into())
    }
}

/// Options for `resign` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct ResignOptions(PbResignRequest);

impl ResignOptions {
    #[inline]
    pub const fn new() -> Self {
        Self(PbResignRequest { leader: None })
    }

    /// The leadership to relinquish by resignation.
    #[inline]
    pub fn with_leader(mut self, leader: LeaderKey) -> Self {
        self.0.leader = Some(leader.into());
        self
    }
}

impl From<ResignOptions> for PbResignRequest {
    #[inline]
    fn from(options: ResignOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbResignRequest> for ResignOptions {
    #[inline]
    fn into_request(self) -> Request<PbResignRequest> {
        Request::new(self.into())
    }
}

/// Response for `Campaign` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct CampaignResponse(PbCampaignResponse);

impl CampaignResponse {
    #[inline]
    const fn new(resp: PbCampaignResponse) -> Self {
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

    /// Describes the resources used for holding leadership of the election.
    #[inline]
    pub fn leader(&self) -> Option<&LeaderKey> {
        self.0.leader.as_ref().map(From::from)
    }

    /// Takes the leader out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_leader(&mut self) -> Option<LeaderKey> {
        self.0.leader.take().map(From::from)
    }
}

/// Response for `Proclaim` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct ProclaimResponse(PbProclaimResponse);

impl ProclaimResponse {
    #[inline]
    const fn new(resp: PbProclaimResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
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

/// Response for `Leader` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct LeaderResponse(PbLeaderResponse);

impl LeaderResponse {
    #[inline]
    const fn new(resp: PbLeaderResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// The key-value pair representing the latest leader update.
    #[inline]
    pub fn kv(&self) -> Option<&KeyValue> {
        self.0.kv.as_ref().map(From::from)
    }

    /// Takes the kv out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_kv(&mut self) -> Option<KeyValue> {
        self.0.kv.take().map(KeyValue::new)
    }
}

/// Response for `Observe` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug)]
pub struct ObserveStream {
    stream: Streaming<PbLeaderResponse>,
}

impl ObserveStream {
    #[inline]
    const fn new(stream: Streaming<PbLeaderResponse>) -> Self {
        Self { stream }
    }

    /// Fetches the next message from this stream.
    #[inline]
    pub async fn message(&mut self) -> Result<Option<LeaderResponse>> {
        match self.stream.message().await? {
            Some(resp) => Ok(Some(LeaderResponse::new(resp))),
            None => Ok(None),
        }
    }
}

impl Stream for ObserveStream {
    type Item = Result<LeaderResponse>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.get_mut().stream)
            .poll_next(cx)
            .map(|t| match t {
                Some(Ok(resp)) => Some(Ok(LeaderResponse::new(resp))),
                Some(Err(e)) => Some(Err(From::from(e))),
                None => None,
            })
    }
}

/// Response for `Resign` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct ResignResponse(PbResignResponse);

impl ResignResponse {
    #[inline]
    const fn new(resp: PbResignResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
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

/// Leader key of election
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct LeaderKey(PbLeaderKey);

impl LeaderKey {
    /// Creates a new leader key.
    #[inline]
    pub const fn new() -> Self {
        Self(PbLeaderKey {
            name: Vec::new(),
            key: Vec::new(),
            rev: 0,
            lease: 0,
        })
    }

    /// The election identifier that corresponds to the leadership key.
    #[inline]
    pub fn with_name(mut self, name: impl Into<Vec<u8>>) -> Self {
        self.0.name = name.into();
        self
    }

    /// An opaque key representing the ownership of the election.
    #[inline]
    pub fn with_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.0.key = key.into();
        self
    }

    /// The creation revision of the key
    #[inline]
    pub const fn with_rev(mut self, rev: i64) -> Self {
        self.0.rev = rev;
        self
    }

    /// The lease ID of the election leader.
    #[inline]
    pub const fn with_lease(mut self, lease: i64) -> Self {
        self.0.lease = lease;
        self
    }

    /// The name in byte. name is the election identifier that corresponds to the leadership key.
    #[inline]
    pub fn name(&self) -> &[u8] {
        &self.0.name
    }

    /// The name in string. name is the election identifier that corresponds to the leadership key.
    #[inline]
    pub fn name_str(&self) -> Result<&str> {
        std::str::from_utf8(self.name()).map_err(From::from)
    }

    /// The name in string. name is the election identifier that corresponds to the leadership key.
    ///
    /// # Safety
    /// This function is unsafe because it does not check that the bytes of the key are valid UTF-8.
    /// If this constraint is violated, undefined behavior results,
    /// as the rest of Rust assumes that [`&str`]s are valid UTF-8.
    #[inline]
    pub unsafe fn name_str_unchecked(&self) -> &str {
        std::str::from_utf8_unchecked(self.name())
    }

    /// The key in byte. key is an opaque key representing the ownership of the election. If the key
    /// is deleted, then leadership is lost.
    #[inline]
    pub fn key(&self) -> &[u8] {
        &self.0.key
    }

    /// The key in string. key is an opaque key representing the ownership of the election. If the key
    /// is deleted, then leadership is lost.
    #[inline]
    pub fn key_str(&self) -> Result<&str> {
        std::str::from_utf8(self.key()).map_err(From::from)
    }

    /// The key in string. key is an opaque key representing the ownership of the election. If the key
    /// is deleted, then leadership is lost.
    ///
    /// # Safety
    /// This function is unsafe because it does not check that the bytes of the key are valid UTF-8.
    /// If this constraint is violated, undefined behavior results,
    /// as the rest of Rust assumes that [`&str`]s are valid UTF-8.
    #[inline]
    pub unsafe fn key_str_unchecked(&self) -> &str {
        std::str::from_utf8_unchecked(self.key())
    }

    /// The creation revision of the key.  It can be used to test for ownership
    /// of an election during transactions by testing the key's creation revision
    /// matches rev.
    #[inline]
    pub const fn rev(&self) -> i64 {
        self.0.rev
    }

    /// The lease ID of the election leader.
    #[inline]
    pub const fn lease(&self) -> i64 {
        self.0.lease
    }
}

impl Default for LeaderKey {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl From<LeaderKey> for PbLeaderKey {
    #[inline]
    fn from(leader_key: LeaderKey) -> Self {
        leader_key.0
    }
}

impl From<PbLeaderKey> for LeaderKey {
    #[inline]
    fn from(key: PbLeaderKey) -> Self {
        Self(key)
    }
}

impl From<&PbLeaderKey> for &LeaderKey {
    #[inline]
    fn from(src: &PbLeaderKey) -> Self {
        unsafe { &*(src as *const _ as *const LeaderKey) }
    }
}

impl ElectionClient {
    /// Creates a election
    #[inline]
    pub(crate) fn new(
        channel: InterceptedChannel,
        auth_token: Arc<RwLock<Option<HeaderValue>>>,
    ) -> Self {
        let inner = PbElectionClient::new(AuthService::new(channel, auth_token));
        Self { inner }
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
        let resp = self
            .inner
            .campaign(
                CampaignOptions::new()
                    .with_name(name)
                    .with_value(value)
                    .with_lease(lease),
            )
            .await?
            .into_inner();
        Ok(CampaignResponse::new(resp))
    }

    /// Lets the leader announce a new value without another election.
    #[inline]
    pub async fn proclaim(
        &mut self,
        value: impl Into<Vec<u8>>,
        options: Option<ProclaimOptions>,
    ) -> Result<ProclaimResponse> {
        let resp = self
            .inner
            .proclaim(options.unwrap_or_default().with_value(value))
            .await?
            .into_inner();
        Ok(ProclaimResponse::new(resp))
    }

    /// Returns the leader value for the current election.
    #[inline]
    pub async fn leader(&mut self, name: impl Into<Vec<u8>>) -> Result<LeaderResponse> {
        let resp = self
            .inner
            .leader(LeaderOptions::new().with_name(name))
            .await?
            .into_inner();
        Ok(LeaderResponse::new(resp))
    }

    /// Returns a channel that reliably observes ordered leader proposals
    /// as GetResponse values on every current elected leader key.
    #[inline]
    pub async fn observe(&mut self, name: impl Into<Vec<u8>>) -> Result<ObserveStream> {
        let resp = self
            .inner
            .observe(LeaderOptions::new().with_name(name))
            .await?
            .into_inner();

        Ok(ObserveStream::new(resp))
    }

    /// Releases election leadership and then start a new election
    #[inline]
    pub async fn resign(&mut self, option: Option<ResignOptions>) -> Result<ResignResponse> {
        let resp = self
            .inner
            .resign(option.unwrap_or_default())
            .await?
            .into_inner();
        Ok(ResignResponse::new(resp))
    }
}
