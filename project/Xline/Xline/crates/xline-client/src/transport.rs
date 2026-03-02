use std::{
    sync::Arc,
    task::{Context, Poll},
};

use http::{HeaderName, HeaderValue, Request};
use tonic::transport::Channel;
use tower::Service;
use tracing::warn;
use xlinerpc::MetaData;

/// Primary header key read by the server-side `authstore::get_token`.
const TOKEN_HEADER: &str = "token";

/// Fallback header key also read by `authstore::get_token`.
///
/// **Non-standard value format**: the server reads the raw JWT token string
/// directly from this header (i.e. `"authorization: <token>"`), **not** the
/// RFC 7235 `"Bearer <token>"` format.  This is intentional and verified
/// against `xline/src/server/auth_server.rs::get_token`, which calls
/// `.to_str()` on the header value without stripping any scheme prefix.
/// Do **not** change to `"Bearer {token}"` without a matching server-side
/// update, as that would break authentication for all existing clients.
const AUTHORIZATION_HEADER: &str = "authorization";

/// Tower [`Service`] wrapper that injects `xlinerpc` metadata into every
/// outgoing HTTP request.
///
/// The metadata is built once at construction time and stored as an
/// `Arc<MetaData>` so that cloning the service is cheap.
///
/// Two header keys are written on each request so that the transition from
/// the old tonic `AuthService` to the new transport layer is seamless:
/// - `token` – the key consumed by the server-side `authstore` extractor.
/// - `authorization` – the key consumed by the legacy tonic auth path.
#[derive(Debug, Clone)]
pub(crate) struct MetadataService<S> {
    /// The inner service that actually sends the request.
    inner: S,
    /// Pre-built metadata to inject; `None` when no authentication is needed.
    metadata: Option<Arc<MetaData>>,
}

impl<S> MetadataService<S> {
    /// Create a new `MetadataService`.
    ///
    /// When `token` is `Some`, both the `token` and `authorization` headers
    /// are written into every outgoing request so that both the authstore and
    /// the legacy tonic auth path can extract the credential.
    #[inline]
    #[must_use]
    pub(crate) fn new(inner: S, token: Option<&str>) -> Self {
        let metadata = token.map(|t| {
            let mut meta = MetaData::new();
            meta.insert(TOKEN_HEADER, t);
            meta.insert(AUTHORIZATION_HEADER, t);
            Arc::new(meta)
        });
        Self { inner, metadata }
    }
}

impl<S, Body, Response> Service<Request<Body>> for MetadataService<S>
where
    S: Service<Request<Body>, Response = Response>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    #[inline]
    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        if let Some(meta) = self.metadata.as_ref() {
            for (key, value) in meta.iter() {
                match (HeaderName::from_bytes(key), HeaderValue::from_bytes(value)) {
                    (Ok(name), Ok(header_value)) => {
                        let _: Option<HeaderValue> =
                            request.headers_mut().insert(name, header_value);
                    }
                    (Err(e), _) => {
                        warn!("skipping auth header: invalid header name: {e}");
                    }
                    (_, Err(e)) => {
                        warn!("skipping auth header: invalid header value: {e}");
                    }
                }
            }
        }
        self.inner.call(request)
    }
}

/// Shared tonic channel wrapper that injects per-request auth metadata.
///
/// This type replaces the old `AuthService<Channel>` used throughout the
/// xline-client sub-clients.  It uses `xlinerpc::MetaData` for header
/// injection, making it straightforward to extend towards QUIC-based
/// transports in the future.
pub(crate) type RpcTransport = MetadataService<Channel>;

/// Build a metadata-aware tonic transport from an existing `Channel`.
///
/// When `token` is `Some`, the resulting transport will inject both the
/// `token` and `authorization` headers on every outgoing RPC so that the
/// server-side authstore can authenticate the caller.
#[inline]
#[must_use]
pub(crate) fn new_rpc_transport(channel: Channel, token: Option<&str>) -> RpcTransport {
    MetadataService::new(channel, token)
}

#[cfg(feature = "quic")]
pub(crate) use quic::QuicXlineTransport;

/// QUIC-based transport for xline API direct RPCs.
///
/// This module provides `QuicXlineTransport`, which replaces the HTTP-header
/// injection path used by `MetadataService` with QUIC frame-level metadata.
/// The auth token is inserted into the `meta: Vec<(String, String)>` field
/// of the frame header instead of an HTTP `Authorization` / `token` header,
/// following the `gm-quic` frame protocol defined in `curp/rpc/quic_transport`.
#[cfg(feature = "quic")]
mod quic {
    use std::{pin::Pin, sync::Arc, time::Duration};

    use curp::rpc::quic_transport::{MethodId, QuicChannel};
    use futures::{Stream, StreamExt as _};
    use prost::Message;
    use xlineapi::{
        AlarmRequest, AlarmResponse, LeaseTimeToLiveRequest, LeaseTimeToLiveResponse,
        LeaseLeasesResponse, MemberAddRequest, MemberAddResponse, MemberListResponse,
        MemberPromoteRequest, MemberPromoteResponse, MemberRemoveRequest, MemberRemoveResponse,
        MemberUpdateRequest, MemberUpdateResponse, SnapshotRequest, StatusRequest, StatusResponse,
    };

    use crate::{
        error::{Result, XlineClientError},
        types::stream::SnapshotStream,
    };
    use xlineapi::command::Command;

    /// Default per-call timeout for QUIC RPC calls.
    const DEFAULT_QUIC_TIMEOUT: Duration = Duration::from_secs(10);

    /// QUIC-based transport for xline API direct RPC calls.
    ///
    /// Wraps a shared `QuicChannel` and pre-builds the frame metadata from
    /// the optional auth token so that every call carries the credential in
    /// the QUIC frame header rather than in an HTTP `Authorization` header.
    ///
    /// Methods mirror the direct-RPC subset of the xline API:
    /// authenticate, lease TTL/list, snapshot, alarm, status, cluster membership.
    /// CURP-proposed calls (put, delete, lease grant/revoke, …) continue to
    /// use the CURP client and are not handled here.
    #[derive(Clone, Debug)]
    pub(crate) struct QuicXlineTransport {
        /// Shared QUIC channel used for all outgoing calls.
        channel: Arc<QuicChannel>,
        /// Pre-built frame-header metadata (token + authorization entries).
        meta: Vec<(String, String)>,
        /// Per-call timeout.
        timeout: Duration,
    }

    impl QuicXlineTransport {
        /// Create a new `QuicXlineTransport`.
        ///
        /// When `token` is `Some`, both `"token"` and `"authorization"` entries
        /// are added to the QUIC frame metadata so that the server-side authstore
        /// can authenticate the caller regardless of which key it reads.
        ///
        /// The `"authorization"` value is the **raw JWT token** (not
        /// `"Bearer <token>"`), matching the behaviour of the tonic/HTTP-2 path
        /// and the server-side `get_token` implementation in
        /// `xline/src/server/auth_server.rs`, which reads the header value with
        /// `.to_str()` and no scheme prefix stripping.
        #[inline]
        #[must_use]
        pub(crate) fn new(channel: Arc<QuicChannel>, token: Option<&str>) -> Self {
            let meta = token
                .map(|t| {
                    vec![
                        ("token".to_owned(), t.to_owned()),
                        // Raw token value – see doc-comment above for why this
                        // intentionally omits the "Bearer " prefix.
                        ("authorization".to_owned(), t.to_owned()),
                    ]
                })
                .unwrap_or_default();
            Self {
                channel,
                meta,
                timeout: DEFAULT_QUIC_TIMEOUT,
            }
        }

        /// Override the per-call timeout.
        #[inline]
        #[must_use]
        pub(crate) fn with_timeout(mut self, timeout: Duration) -> Self {
            self.timeout = timeout;
            self
        }

        /// Perform a unary QUIC call and decode the response.
        ///
        /// The auth token is passed via frame metadata (`self.meta`), not as
        /// an HTTP header.
        async fn unary<Req, Resp>(&self, method: MethodId, req: Req) -> Result<Resp>
        where
            Req: Message,
            Resp: Message + Default,
        {
            self.channel
                .unary_call(method, req, self.meta.clone(), self.timeout)
                .await
                .map_err(|e| XlineClientError::<Command>::RpcError(e.to_string()))
        }

        /// Authenticate with the xline server using username and password.
        ///
        /// Uses `MethodId::XlineAuthenticate` over QUIC frame protocol.
        #[inline]
        pub(crate) async fn authenticate(
            &self,
            name: String,
            password: String,
        ) -> Result<xlineapi::AuthenticateResponse> {
            self.unary(
                MethodId::XlineAuthenticate,
                xlineapi::AuthenticateRequest { name, password },
            )
            .await
        }

        /// Query the TTL of a lease.
        ///
        /// Uses `MethodId::XlineLeaseTtl` over QUIC frame protocol.
        #[inline]
        pub(crate) async fn lease_time_to_live(
            &self,
            id: i64,
            keys: bool,
        ) -> Result<LeaseTimeToLiveResponse> {
            self.unary(
                MethodId::XlineLeaseTtl,
                LeaseTimeToLiveRequest { id, keys },
            )
            .await
        }

        /// List all active leases on the server.
        ///
        /// Uses `MethodId::XlineLeaseLeases` over QUIC frame protocol.
        #[inline]
        pub(crate) async fn lease_leases(&self) -> Result<LeaseLeasesResponse> {
            self.unary(MethodId::XlineLeaseLeases, xlineapi::LeaseLeasesRequest {})
                .await
        }

        /// Request a snapshot stream from the server.
        ///
        /// Uses `MethodId::XlineSnapshot` with server-streaming over QUIC.
        /// Each `CurpError` from the inner stream is converted to an
        /// `xlinerpc::Status` with code `Internal`.
        #[inline]
        pub(crate) async fn snapshot(&self) -> Result<SnapshotStream> {
            let stream = self
                .channel
                .server_streaming_call(
                    MethodId::XlineSnapshot,
                    SnapshotRequest {},
                    self.meta.clone(),
                    self.timeout,
                )
                .await
                .map_err(|e| XlineClientError::<Command>::RpcError(e.to_string()))?;

            let converted = stream.map(|item| {
                item.map_err(|e| {
                    xlinerpc::Status::new(xlinerpc::Code::Internal, e.to_string())
                })
            });
            Ok(SnapshotStream::new(converted))
        }

        /// Send an alarm request to the server.
        ///
        /// Uses `MethodId::XlineAlarm` over QUIC frame protocol.
        #[inline]
        pub(crate) async fn alarm(&self, req: AlarmRequest) -> Result<AlarmResponse> {
            self.unary(MethodId::XlineAlarm, req).await
        }

        /// Query the maintenance status of the server.
        ///
        /// Uses `MethodId::XlineMaintStatus` over QUIC frame protocol.
        #[inline]
        pub(crate) async fn maint_status(&self) -> Result<StatusResponse> {
            self.unary(MethodId::XlineMaintStatus, StatusRequest {}).await
        }

        /// Add a new member to the cluster.
        ///
        /// Uses `MethodId::XlineMemberAdd` over QUIC frame protocol.
        #[inline]
        pub(crate) async fn member_add(&self, req: MemberAddRequest) -> Result<MemberAddResponse> {
            self.unary(MethodId::XlineMemberAdd, req).await
        }

        /// Remove a member from the cluster.
        ///
        /// Uses `MethodId::XlineMemberRemove` over QUIC frame protocol.
        #[inline]
        pub(crate) async fn member_remove(
            &self,
            id: u64,
        ) -> Result<MemberRemoveResponse> {
            self.unary(MethodId::XlineMemberRemove, MemberRemoveRequest { id })
                .await
        }

        /// Promote a learner member to a voting member.
        ///
        /// Uses `MethodId::XlineMemberPromote` over QUIC frame protocol.
        #[inline]
        pub(crate) async fn member_promote(&self, id: u64) -> Result<MemberPromoteResponse> {
            self.unary(MethodId::XlineMemberPromote, MemberPromoteRequest { id })
                .await
        }

        /// Update the peer URLs of an existing cluster member.
        ///
        /// Uses `MethodId::XlineMemberUpdate` over QUIC frame protocol.
        #[inline]
        pub(crate) async fn member_update(&self, req: MemberUpdateRequest) -> Result<MemberUpdateResponse> {
            self.unary(MethodId::XlineMemberUpdate, req).await
        }

        /// List all cluster members.
        ///
        /// Uses `MethodId::XlineMemberList` over QUIC frame protocol.
        #[inline]
        pub(crate) async fn member_list(
            &self,
            linearizable: bool,
        ) -> Result<MemberListResponse> {
            self.unary(
                MethodId::XlineMemberList,
                xlineapi::MemberListRequest { linearizable },
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use futures::future::{Ready, ready};
    use tower::Service;

    use super::MetadataService;

    #[derive(Debug, Clone)]
    struct EchoService;

    impl Service<http::Request<()>> for EchoService {
        type Response = http::Request<()>;
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: http::Request<()>) -> Self::Future {
            ready(Ok(request))
        }
    }

    #[tokio::test]
    async fn metadata_service_injects_token_and_authorization_headers() {
        let mut svc = MetadataService::new(EchoService, Some("test-token"));
        let req = http::Request::new(());
        let req = svc.call(req).await.expect("service should not fail");

        assert_eq!(
            req.headers()
                .get("token")
                .and_then(|v| v.to_str().ok())
                .expect("missing token header"),
            "test-token"
        );
        assert_eq!(
            req.headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .expect("missing authorization header"),
            "test-token"
        );
    }

    #[tokio::test]
    async fn metadata_service_no_headers_when_no_token() {
        let mut svc = MetadataService::new(EchoService, None);
        let req = http::Request::new(());
        let req = svc.call(req).await.expect("service should not fail");

        assert!(req.headers().get("token").is_none());
        assert!(req.headers().get("authorization").is_none());
    }
}
