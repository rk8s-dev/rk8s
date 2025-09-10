//! Etcd Lock RPC.

use super::pb::v3lockpb;
use crate::auth::AuthService;
use crate::error::Result;
use crate::intercept::InterceptedChannel;
use crate::rpc::ResponseHeader;
use http::HeaderValue;
use std::sync::{Arc, RwLock};
use tonic::{IntoRequest, Request};
use v3lockpb::lock_client::LockClient as PbLockClient;
use v3lockpb::{
    LockRequest as PbLockRequest, LockResponse as PbLockResponse, UnlockRequest as PbUnlockRequest,
    UnlockResponse as PbUnlockResponse,
};

/// Client for Lock operations.
#[repr(transparent)]
#[derive(Clone)]
pub struct LockClient {
    inner: PbLockClient<AuthService<InterceptedChannel>>,
}

impl LockClient {
    /// Creates a lock client.
    #[inline]
    pub(crate) fn new(
        channel: InterceptedChannel,
        auth_token: Arc<RwLock<Option<HeaderValue>>>,
    ) -> Self {
        let inner = PbLockClient::new(AuthService::new(channel, auth_token));
        Self { inner }
    }

    /// Acquires a distributed shared lock on a given named lock.
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
        let resp = self
            .inner
            .lock(options.unwrap_or_default().with_name(name))
            .await?
            .into_inner();
        Ok(LockResponse::new(resp))
    }

    /// Takes a key returned by Lock and releases the hold on lock. The
    /// next Lock caller waiting for the lock will then be woken up and given
    /// ownership of the lock.
    #[inline]
    pub async fn unlock(&mut self, key: impl Into<Vec<u8>>) -> Result<UnlockResponse> {
        let resp = self
            .inner
            .unlock(UnlockOptions::new().with_key(key))
            .await?
            .into_inner();
        Ok(UnlockResponse::new(resp))
    }
}

/// Options for `Lock` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct LockOptions(PbLockRequest);

impl LockOptions {
    /// name is the identifier for the distributed shared lock to be acquired.
    #[inline]
    fn with_name(mut self, name: impl Into<Vec<u8>>) -> Self {
        self.0.name = name.into();
        self
    }

    /// Creates a `LockOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbLockRequest {
            name: Vec::new(),
            lease: 0,
        })
    }

    /// `lease` is the ID of the lease that will be attached to ownership of the
    /// lock. If the lease expires or is revoked and currently holds the lock,
    /// the lock is automatically released. Calls to Lock with the same lease will
    /// be treated as a single acquisition; locking twice with the same lease is a
    /// no-op.
    #[inline]
    pub const fn with_lease(mut self, lease: i64) -> Self {
        self.0.lease = lease;
        self
    }
}

impl From<LockOptions> for PbLockRequest {
    #[inline]
    fn from(options: LockOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbLockRequest> for LockOptions {
    #[inline]
    fn into_request(self) -> Request<PbLockRequest> {
        Request::new(self.into())
    }
}

/// Response for `Lock` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct LockResponse(PbLockResponse);

impl LockResponse {
    /// Create a new `LockResponse` from pb lock response.
    #[inline]
    const fn new(resp: PbLockResponse) -> Self {
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

    /// A key that will exist on etcd for the duration that the Lock caller
    /// owns the lock. Users should not modify this key or the lock may exhibit
    /// undefined behavior.
    #[inline]
    pub fn key(&self) -> &[u8] {
        &self.0.key
    }
}

/// Options for `Unlock` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct UnlockOptions(PbUnlockRequest);

impl UnlockOptions {
    /// key is the lock ownership key granted by Lock.
    #[inline]
    fn with_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.0.key = key.into();
        self
    }

    /// Creates a `UnlockOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbUnlockRequest { key: Vec::new() })
    }
}

impl From<UnlockOptions> for PbUnlockRequest {
    #[inline]
    fn from(options: UnlockOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbUnlockRequest> for UnlockOptions {
    #[inline]
    fn into_request(self) -> Request<PbUnlockRequest> {
        Request::new(self.into())
    }
}

/// Response for `Unlock` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct UnlockResponse(PbUnlockResponse);

impl UnlockResponse {
    /// Create a new `UnlockResponse` from pb unlock response.
    #[inline]
    const fn new(resp: PbUnlockResponse) -> Self {
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
