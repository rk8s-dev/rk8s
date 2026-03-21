// use crate::{
// body::{boxed, BoxBody},
// metadata::GRPC_CONTENT_TYPE,
// server::NamedService,
// Status,
// };
use super::{Body, Error as GlobalError, HeaderValue, Router, h3wrapper::QuicIncomingBody};
use bytes::Bytes;
use gm_quic::prelude::{BindUri, ParseBindUriError, QuicListeners, handy};
use h3::{
    quic::{BidiStream, SendStream},
    server::RequestStream,
};
use h3_shim;
use http::{Request, Response};
use std::{collections::HashMap, convert::Infallible, future::poll_fn, sync::Arc};
use tower::Service;
use utils::config::TlsConfig;
// use anyhow::Result;

/// A Server for creating axum routers for gRPC services
#[derive(Debug, Default, Clone)]
pub struct RouterBuilder {
    router: Router,
    tls_config: TlsConfig,
}

impl RouterBuilder {
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
}

pub(crate) struct Server {
    // servername: (router, peer_urls)
    routers: HashMap<String, (RouterBuilder, Vec<String>)>,
}

impl Server {
    pub(crate) fn new() -> Self {
        Server {
            routers: HashMap::new(),
        }
    }

    /// Add a router nested to the router
    pub(crate) fn add_server(
        mut self,
        name: &str,
        router: RouterBuilder,
        peer_urls: impl IntoIterator<Item = String>,
    ) -> Self {
        if let Some(_) = self
            .routers
            .insert(name.to_string(), (router, peer_urls.into_iter().collect()))
        {
            panic!("{}", format!("duplicate server name {name}"));
        }
        self
    }

    pub(crate) async fn serve(self) -> Result<(), super::Error> {
        let listeners = QuicListeners::builder().map(|builder| {
            builder
                .without_client_cert_verifier()
                .with_parameters(handy::server_parameters())
                .with_alpns(["h3"])
                .listen(4096)
        })?;
        for (server_name, (router_builder, peer_urls)) in &self.routers {
            listeners.add_server(
                server_name,
                router_builder
                    .tls_config
                    .peer_cert_path()
                    .clone()
                    .expect("server tls cert config is needed")
                    .as_path(),
                router_builder
                    .tls_config
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

            let _ = listeners
                .get_server(&server_name)
                .unwrap()
                .bind_interfaces()
                .iter()
                .next()
                .unwrap()
                .1
                .borrow()?;
        }

        // tracing::info!("yes quic is serving");
        // handle incoming connections and requests
        while let Ok((new_conn, server, _pathway, _link)) = listeners.accept().await {
            tracing::info!("get a connection  {server:?} {_pathway:?} {_link:?}");
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
            if let Some((RouterBuilder { router, .. }, _)) = self.routers.get(&server) {
                let _ = tokio::spawn(Self::handle_connection(router.clone(), h3_conn));
            }
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
                Ok(None) => {
                    tracing::error!("failed to accept a conenction");
                    break;
                }
                Err(_) => {
                    // tracing::error!("encounter an error: {e:?}");
                    break;
                }
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
    let (parts, _) = request.into_parts();
    let resp = service.call(Request::from_parts(parts, body)).await?;
    let (parts, body) = resp.into_parts();
    send.send_response(Response::from_parts(parts, ())).await?;
    copy_response_body(send, body).await?;
    Ok(())
}
#[allow(unused)]
async fn unimplemented() -> impl axum::response::IntoResponse {
    tracing::error!("unimplemented");
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
            Err(frame) => {
                if let Ok(trailers) = frame.into_trailers() {
                    send.send_trailers(trailers).await?;
                }
                continue;
            }
        }
    }

    send.finish().await?;

    Ok(())
}
