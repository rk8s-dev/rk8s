//! Etcd Lease RPC.

use crate::auth::AuthService;
use crate::error::Result;
use crate::intercept::InterceptedChannel;
use crate::rpc::pb::etcdserverpb::lease_client::LeaseClient as PbLeaseClient;
use crate::rpc::pb::etcdserverpb::{
    LeaseGrantRequest as PbLeaseGrantRequest, LeaseGrantResponse as PbLeaseGrantResponse,
    LeaseKeepAliveRequest as PbLeaseKeepAliveRequest,
    LeaseKeepAliveResponse as PbLeaseKeepAliveResponse, LeaseLeasesRequest as PbLeaseLeasesRequest,
    LeaseLeasesResponse as PbLeaseLeasesResponse, LeaseRevokeRequest as PbLeaseRevokeRequest,
    LeaseRevokeResponse as PbLeaseRevokeResponse, LeaseStatus as PbLeaseStatus,
    LeaseTimeToLiveRequest as PbLeaseTimeToLiveRequest,
    LeaseTimeToLiveResponse as PbLeaseTimeToLiveResponse,
};
use crate::rpc::ResponseHeader;
use crate::vec::VecExt;
use crate::Error;
use http::HeaderValue;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use tokio::sync::mpsc::{channel, Sender};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tonic::{IntoRequest, Request, Streaming};

/// Client for lease operations.
#[repr(transparent)]
#[derive(Clone)]
pub struct LeaseClient {
    inner: PbLeaseClient<AuthService<InterceptedChannel>>,
}

impl LeaseClient {
    /// Creates a `LeaseClient`.
    #[inline]
    pub(crate) fn new(
        channel: InterceptedChannel,
        auth_token: Arc<RwLock<Option<HeaderValue>>>,
    ) -> Self {
        let inner = PbLeaseClient::new(AuthService::new(channel, auth_token));
        Self { inner }
    }

    /// Creates a lease which expires if the server does not receive a keepAlive
    /// within a given time to live period. All keys attached to the lease will be expired and
    /// deleted if the lease expires. Each expired key generates a delete event in the event history.
    #[inline]
    pub async fn grant(
        &mut self,
        ttl: i64,
        options: Option<LeaseGrantOptions>,
    ) -> Result<LeaseGrantResponse> {
        let resp = self
            .inner
            .lease_grant(options.unwrap_or_default().with_ttl(ttl))
            .await?
            .into_inner();
        Ok(LeaseGrantResponse::new(resp))
    }

    /// Revokes a lease. All keys attached to the lease will expire and be deleted.
    #[inline]
    pub async fn revoke(&mut self, id: i64) -> Result<LeaseRevokeResponse> {
        let resp = self
            .inner
            .lease_revoke(LeaseRevokeOptions::new().with_id(id))
            .await?
            .into_inner();
        Ok(LeaseRevokeResponse::new(resp))
    }

    /// Keeps the lease alive by streaming keep alive requests from the client
    /// to the server and streaming keep alive responses from the server to the client.
    #[inline]
    pub async fn keep_alive(&mut self, id: i64) -> Result<(LeaseKeeper, LeaseKeepAliveStream)> {
        let (sender, receiver) = channel::<PbLeaseKeepAliveRequest>(100);
        sender
            .send(LeaseKeepAliveOptions::new().with_id(id).into())
            .await
            .map_err(|e| Error::LeaseKeepAliveError(e.to_string()))?;

        let receiver = ReceiverStream::new(receiver);

        let mut stream = self.inner.lease_keep_alive(receiver).await?.into_inner();

        let id = match stream.message().await? {
            Some(resp) => {
                if resp.ttl <= 0 {
                    return Err(Error::LeaseKeepAliveError("lease not found".to_string()));
                }
                resp.id
            }
            None => {
                return Err(Error::WatchError(
                    "failed to create lease keeper".to_string(),
                ));
            }
        };

        Ok((
            LeaseKeeper::new(id, sender),
            LeaseKeepAliveStream::new(stream),
        ))
    }

    /// Retrieves lease information.
    #[inline]
    pub async fn time_to_live(
        &mut self,
        id: i64,
        options: Option<LeaseTimeToLiveOptions>,
    ) -> Result<LeaseTimeToLiveResponse> {
        let resp = self
            .inner
            .lease_time_to_live(options.unwrap_or_default().with_id(id))
            .await?
            .into_inner();
        Ok(LeaseTimeToLiveResponse::new(resp))
    }

    /// Lists all existing leases.
    #[inline]
    pub async fn leases(&mut self) -> Result<LeaseLeasesResponse> {
        let resp = self
            .inner
            .lease_leases(PbLeaseLeasesRequest {})
            .await?
            .into_inner();
        Ok(LeaseLeasesResponse::new(resp))
    }
}

/// Options for `Grant` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct LeaseGrantOptions(PbLeaseGrantRequest);

impl LeaseGrantOptions {
    /// Set ttl
    #[inline]
    const fn with_ttl(mut self, ttl: i64) -> Self {
        self.0.ttl = ttl;
        self
    }

    /// Set id
    #[inline]
    pub const fn with_id(mut self, id: i64) -> Self {
        self.0.id = id;
        self
    }

    /// Creates a `LeaseGrantOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbLeaseGrantRequest { ttl: 0, id: 0 })
    }
}

impl From<LeaseGrantOptions> for PbLeaseGrantRequest {
    #[inline]
    fn from(options: LeaseGrantOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbLeaseGrantRequest> for LeaseGrantOptions {
    #[inline]
    fn into_request(self) -> Request<PbLeaseGrantRequest> {
        Request::new(self.into())
    }
}

/// Response for `Grant` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct LeaseGrantResponse(PbLeaseGrantResponse);

impl LeaseGrantResponse {
    /// Creates a new `LeaseGrantResponse` from pb lease grant response.
    #[inline]
    const fn new(resp: PbLeaseGrantResponse) -> Self {
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

    /// TTL is the server chosen lease time-to-live in seconds
    #[inline]
    pub const fn ttl(&self) -> i64 {
        self.0.ttl
    }

    /// ID is the lease ID for the granted lease.
    #[inline]
    pub const fn id(&self) -> i64 {
        self.0.id
    }

    /// Error message if return error.
    #[inline]
    pub fn error(&self) -> &str {
        &self.0.error
    }
}

/// Options for `Revoke` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
struct LeaseRevokeOptions(PbLeaseRevokeRequest);

impl LeaseRevokeOptions {
    /// Set id
    #[inline]
    fn with_id(mut self, id: i64) -> Self {
        self.0.id = id;
        self
    }

    /// Creates a `LeaseRevokeOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbLeaseRevokeRequest { id: 0 })
    }
}

impl From<LeaseRevokeOptions> for PbLeaseRevokeRequest {
    #[inline]
    fn from(options: LeaseRevokeOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbLeaseRevokeRequest> for LeaseRevokeOptions {
    #[inline]
    fn into_request(self) -> Request<PbLeaseRevokeRequest> {
        Request::new(self.into())
    }
}

/// Response for `Revoke` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct LeaseRevokeResponse(PbLeaseRevokeResponse);

impl LeaseRevokeResponse {
    /// Creates a new `LeaseRevokeResponse` from pb lease revoke response.
    #[inline]
    const fn new(resp: PbLeaseRevokeResponse) -> Self {
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

/// Options for `KeepAlive` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
struct LeaseKeepAliveOptions(PbLeaseKeepAliveRequest);

impl LeaseKeepAliveOptions {
    /// Set id
    #[inline]
    fn with_id(mut self, id: i64) -> Self {
        self.0.id = id;
        self
    }

    /// Creates a `LeaseKeepAliveOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbLeaseKeepAliveRequest { id: 0 })
    }
}

impl From<LeaseKeepAliveOptions> for PbLeaseKeepAliveRequest {
    #[inline]
    fn from(options: LeaseKeepAliveOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbLeaseKeepAliveRequest> for LeaseKeepAliveOptions {
    #[inline]
    fn into_request(self) -> Request<PbLeaseKeepAliveRequest> {
        Request::new(self.into())
    }
}

/// Response for `KeepAlive` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct LeaseKeepAliveResponse(PbLeaseKeepAliveResponse);

impl LeaseKeepAliveResponse {
    /// Creates a new `LeaseKeepAliveResponse` from pb lease KeepAlive response.
    #[inline]
    const fn new(resp: PbLeaseKeepAliveResponse) -> Self {
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

    /// TTL is the new time-to-live for the lease.
    #[inline]
    pub const fn ttl(&self) -> i64 {
        self.0.ttl
    }

    /// ID is the lease ID for the keep alive request.
    #[inline]
    pub const fn id(&self) -> i64 {
        self.0.id
    }
}

/// Options for `TimeToLive` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct LeaseTimeToLiveOptions(PbLeaseTimeToLiveRequest);

impl LeaseTimeToLiveOptions {
    /// ID is the lease ID for the lease.
    #[inline]
    const fn with_id(mut self, id: i64) -> Self {
        self.0.id = id;
        self
    }

    /// Keys is true to query all the keys attached to this lease.
    #[inline]
    pub const fn with_keys(mut self) -> Self {
        self.0.keys = true;
        self
    }

    /// Creates a `LeaseTimeToLiveOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbLeaseTimeToLiveRequest { id: 0, keys: false })
    }
}

impl From<LeaseTimeToLiveOptions> for PbLeaseTimeToLiveRequest {
    #[inline]
    fn from(options: LeaseTimeToLiveOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbLeaseTimeToLiveRequest> for LeaseTimeToLiveOptions {
    #[inline]
    fn into_request(self) -> Request<PbLeaseTimeToLiveRequest> {
        Request::new(self.into())
    }
}

/// Response for `TimeToLive` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct LeaseTimeToLiveResponse(PbLeaseTimeToLiveResponse);

impl LeaseTimeToLiveResponse {
    /// Creates a new `LeaseTimeToLiveResponse` from pb lease TimeToLive response.
    #[inline]
    const fn new(resp: PbLeaseTimeToLiveResponse) -> Self {
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

    /// TTL is the remaining TTL in seconds for the lease; the lease will expire in under TTL+1 seconds.
    #[inline]
    pub const fn ttl(&self) -> i64 {
        self.0.ttl
    }

    /// ID is the lease ID from the keep alive request.
    #[inline]
    pub const fn id(&self) -> i64 {
        self.0.id
    }

    /// GrantedTTL is the initial granted time in seconds upon lease creation/renewal.
    #[inline]
    pub const fn granted_ttl(&self) -> i64 {
        self.0.granted_ttl
    }

    /// Keys is the list of keys attached to this lease.
    #[inline]
    pub fn keys(&self) -> &[Vec<u8>] {
        &self.0.keys
    }

    #[inline]
    pub(crate) fn strip_keys_prefix(&mut self, prefix: &[u8]) {
        self.0.keys.iter_mut().for_each(|key| {
            key.strip_key_prefix(prefix);
        });
    }
}

/// Response for `Leases` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct LeaseLeasesResponse(PbLeaseLeasesResponse);

impl LeaseLeasesResponse {
    /// Creates a new `LeaseLeasesResponse` from pb lease Leases response.
    #[inline]
    const fn new(resp: PbLeaseLeasesResponse) -> Self {
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

    /// Get leases status
    #[inline]
    pub fn leases(&self) -> &[LeaseStatus] {
        unsafe { &*(self.0.leases.as_slice() as *const _ as *const [LeaseStatus]) }
    }
}

/// Lease status.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone, PartialEq)]
#[repr(transparent)]
pub struct LeaseStatus(PbLeaseStatus);

impl LeaseStatus {
    /// Lease id.
    #[inline]
    pub const fn id(&self) -> i64 {
        self.0.id
    }
}

impl From<&PbLeaseStatus> for &LeaseStatus {
    #[inline]
    fn from(src: &PbLeaseStatus) -> Self {
        unsafe { &*(src as *const _ as *const LeaseStatus) }
    }
}

/// The lease keep alive handle.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug)]
pub struct LeaseKeeper {
    id: i64,
    sender: Sender<PbLeaseKeepAliveRequest>,
}

impl LeaseKeeper {
    /// Creates a new `LeaseKeeper`.
    #[inline]
    const fn new(id: i64, sender: Sender<PbLeaseKeepAliveRequest>) -> Self {
        Self { id, sender }
    }

    /// The lease id which user want to keep alive.
    #[inline]
    pub const fn id(&self) -> i64 {
        self.id
    }

    /// Sends a keep alive request and receive response
    #[inline]
    pub async fn keep_alive(&mut self) -> Result<()> {
        self.sender
            .send(LeaseKeepAliveOptions::new().with_id(self.id).into())
            .await
            .map_err(|e| Error::LeaseKeepAliveError(e.to_string()))
    }
}

/// The lease keep alive response stream.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug)]
pub struct LeaseKeepAliveStream {
    stream: Streaming<PbLeaseKeepAliveResponse>,
}

impl LeaseKeepAliveStream {
    /// Creates a new `LeaseKeepAliveStream`.
    #[inline]
    const fn new(stream: Streaming<PbLeaseKeepAliveResponse>) -> Self {
        Self { stream }
    }

    /// Fetches the next message from this stream.
    #[inline]
    pub async fn message(&mut self) -> Result<Option<LeaseKeepAliveResponse>> {
        match self.stream.message().await? {
            Some(resp) => Ok(Some(LeaseKeepAliveResponse::new(resp))),
            None => Ok(None),
        }
    }
}

impl Stream for LeaseKeepAliveStream {
    type Item = Result<LeaseKeepAliveResponse>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.get_mut().stream)
            .poll_next(cx)
            .map(|t| match t {
                Some(Ok(resp)) => Some(Ok(LeaseKeepAliveResponse::new(resp))),
                Some(Err(e)) => Some(Err(From::from(e))),
                None => None,
            })
    }
}
