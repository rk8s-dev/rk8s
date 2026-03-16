// use crate::{
// body::{boxed, BoxBody},
// metadata::GRPC_CONTENT_TYPE,
// server::NamedService,
// Status,
// };
use super::{
    Body, Error as GlobalError, HeaderValue, Router, h3wrapper::QuicIncomingBody,
};
use bytes::Bytes;
use gm_quic::prelude::{BindUri, ParseBindUriError, QuicListeners, handy};
use h3::{
    quic::{BidiStream, SendStream},
    server::RequestStream,
};
use h3_shim;
use http::{Request, Response};
use std::{
    convert::Infallible,
    future::poll_fn,
    sync::Arc,
};
use tower::Service;
use utils::config::TlsConfig;
// use anyhow::Result;

/// A Server for creating axum routers for gRPC services
#[derive(Debug, Default, Clone)]
pub struct Server {
    router: Router,
    tls_config: TlsConfig,
}

impl Server {
    /// Create a new Server with an empty router
    pub fn new() -> Self {
        Self {
            router: Router::new().fallback(unimplemented),
            tls_config: TlsConfig::default(),
        }
    }

    pub fn add_service<S>(mut self, name: &str, svc: S) -> Self
    where
        S: Service<axum::extract::Request, Error = Infallible> + Clone + Send + 'static,
        S::Future: Send + 'static,
        S::Error: Into<super::Error> + Send,
        S::Response: axum::response::IntoResponse,
    {
        self.router = self.router.route_service(name, svc);
        self
    }

    /// Add a router nested to the router
    pub fn add_subrouter(mut self, name: &str, router: Router) -> Self {
        self.router = self.router.nest(name, router);
        self
    }

    /// Finalize the router and return it
    pub fn build(self) -> Router {
        self.router
    }

    /// Optimize the router for performance
    pub fn prepare(mut self) -> Self {
        self.router = self.router.with_state(());
        self
    }

    pub fn tls_config(self, config: &TlsConfig) -> Self {
        Self {
            router: self.router,
            tls_config: config.clone(),
        }
    }

    pub(crate) async fn serve(
        self,
        peer_urls: impl IntoIterator<Item = String>,
    ) -> Result<(), super::Error>
where {
        // let concurrency_limit = self.concurrency_limit;
        // let init_connection_window_size = self.init_connection_window_size;
        // let init_stream_window_size = self.init_stream_window_size;
        // let max_concurrent_streams = self.max_concurrent_streams;
        // let timeout = self.timeout;
        // let max_frame_size = self.max_frame_size;
        // let max_connection_age = self.max_connection_age;

        let listeners = QuicListeners::builder().map(|builder| {
            builder
                .without_client_cert_verifier()
                .with_parameters(handy::server_parameters())
                .listen(4096)
        })?;
        listeners.add_server(
            "localhost",
            self.tls_config
                .peer_cert_path()
                .clone()
                .expect("server tls cert config is needed")
                .as_path(),
            self.tls_config
                .peer_key_path()
                .clone()
                .expect("server tls key config is needed")
                .as_path(),
            peer_urls
                .into_iter()
                .map(|s| s.parse().map_err(|e: ParseBindUriError| anyhow::anyhow!(e)))
                .collect::<anyhow::Result<Vec<BindUri>>>()?,
            None,
        )?;

        // handle incoming connections and requests
        while let Ok((new_conn, _server, _pathway, _link)) = listeners.accept().await {
            let h3_conn =
                match h3::server::Connection::new(h3_shim::QuicConnection::new(Arc::new(new_conn)))
                    .await
                {
                    Ok(h3_conn) => {
                        tracing::info!("Accept a new quic connection");
                        h3_conn
                    }
                    Err(error) => {
                        tracing::error!("Failed to establish h3 connection: {}", error);
                        continue;
                    }
                };
            let _ = tokio::spawn(Self::handle_connection(self.router.clone(), h3_conn));
        }

        Ok(())
    }

    async fn handle_connection<T>(router: Router, mut connection: h3::server::Connection<T, Bytes>)
    where
        T: h3::quic::Connection<Bytes> + 'static,
        <T as h3::quic::OpenStreams<Bytes>>::BidiStream: BidiStream<Bytes> + Send + 'static,
        <<T as h3::quic::OpenStreams<Bytes>>::BidiStream as BidiStream<Bytes>>::RecvStream: Send,
        <<T as h3::quic::OpenStreams<Bytes>>::BidiStream as BidiStream<Bytes>>::SendStream: Send,
    {
        let svc = router.into_service();
        loop {
            match connection.accept().await {
                Ok(Some(request_resolver)) => {
                    let svc = svc.clone();
                    let _ = tokio::spawn(async move {
                        let (request, stream) = request_resolver.resolve_request().await?;
                        let res = handle_request(request, stream, svc).await;
                        res.map_err(|e| {
                            tracing::error!("Handling request failed: {}", e);
                            e
                        })
                    });
                }
                Ok(None) => break,
                Err(..) => break,
            }
        }
    }
}

async fn handle_request<T, SVC, ResBody>(
    request: Request<()>,
    stream: RequestStream<T, Bytes>,
    mut service: SVC,
) -> Result<(), GlobalError>
where
    T: BidiStream<Bytes> + 'static,
    SVC: Service<Request<QuicIncomingBody<T::RecvStream>>, Response = Response<ResBody>>
        + Clone
        + Send
        + 'static,
    SVC::Future: Send + 'static,
    SVC::Error: Into<GlobalError> + Send + Sync + std::error::Error,
    ResBody: Body<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<GlobalError> + Send + Sync + std::error::Error,
{
    poll_fn(|cx| service.poll_ready(cx)).await?;

    let (mut send, recv) = stream.split();
    let body = QuicIncomingBody::new(
        recv,
        request
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|len| len.to_str().ok().and_then(|x| x.parse().ok())),
    );
    let resp = service.call(Request::new(body)).await?;
    let (parts, body) = resp.into_parts();
    send.send_response(Response::from_parts(parts, ())).await?;
    copy_response_body(send, body).await?;
    Ok(())
}
async fn unimplemented() -> impl axum::response::IntoResponse {
    let status = http::StatusCode::OK;
    let headers = [
        (tonic::Status::GRPC_STATUS, HeaderValue::from_static("12")),
        (
            http::header::CONTENT_TYPE,
            tonic::metadata::GRPC_CONTENT_TYPE,
        ),
    ];
    (status, headers)
}

/// Copy the response body to the given stream.
pub(crate) async fn copy_response_body<S, ResBody>(
    mut send: RequestStream<S, Bytes>,
    body: ResBody,
) -> Result<(), GlobalError>
where
    S: SendStream<Bytes>,
    ResBody: Body<Data = Bytes>,
    ResBody::Error: Into<GlobalError> + Send + Sync + std::error::Error + 'static,
{
    let mut body = std::pin::pin!(body);

    while let Some(frame) = poll_fn(|cx| body.as_mut().poll_frame(cx)).await {
        match frame?.into_data() {
            Ok(data) => send.send_data(data).await?,
            Err(_) => continue,
        }
    }

    send.finish().await?;

    Ok(())
}
