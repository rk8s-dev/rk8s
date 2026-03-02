use std::{
    sync::Arc,
    task::{Context, Poll},
};

use http::{HeaderName, HeaderValue, Request};
use tonic::transport::Channel;
use tower::Service;
use xlinerpc::MetaData;

/// Header key extracted by the server-side authstore.
const TOKEN_HEADER: &str = "token";
/// Standard HTTP `Authorization` header – kept for tonic-gRPC compatibility.
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
                if let (Ok(name), Ok(header_value)) =
                    (HeaderName::from_bytes(key), HeaderValue::from_bytes(value))
                {
                    let _: Option<HeaderValue> = request.headers_mut().insert(name, header_value);
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
