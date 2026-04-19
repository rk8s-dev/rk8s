//! H3 server implementation for Xline
//!
//! This module provides the HTTP/3 server functionality specific to Xline,
//! including port-based routing between client requests and CURP peer communication.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bytes::Bytes;
use curp::rpc::QuicGrpcServer;
use gm_quic::prelude::{EndpointAddr, QuicListeners};
use h3::quic::{BidiStream, SendStream};
use h3::server::RequestStream;
use http::{Request, Response};
use tower::{Service, ServiceExt};
use tracing::{debug, error, info, trace, warn};
use xlineapi::command::{Command, CurpClient};

use crate::server::command::CommandExecutor;
use crate::state::State;
use utils::config::TlsConfig;
use xlinerpc::server::{extract_host_from_url, parse_bind_uri};

/// Router builder for Xline H3 server
#[derive(Clone)]
pub(crate) struct RouterBuilder {
    router: xlinerpc::server::StateRouter<()>,
    tls_config: TlsConfig,
}

impl RouterBuilder {
    /// Create a new builder
    pub(crate) fn new() -> Self {
        Self {
            router: xlinerpc::server::StateRouter::new().fallback(unimplemented),
            tls_config: TlsConfig::default(),
        }
    }

    /// Add a nested router
    pub(crate) fn add_subrouter(
        mut self,
        name: &str,
        router: xlinerpc::server::StateRouter<()>,
    ) -> Self {
        self.router = self.router.nest(name, router);
        self
    }

    /// Set TLS config
    pub(crate) fn set_tls_config(mut self, config: &TlsConfig) -> Self {
        self.tls_config = config.clone();
        self
    }

    /// Get the inner router
    pub(crate) fn into_inner(self) -> xlinerpc::server::StateRouter<()> {
        self.router
    }
}

impl Default for RouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Global shared state for the QUIC accept loop.
struct SharedQuicState {
    listeners: Arc<QuicListeners>,
    servers: Arc<tokio::sync::RwLock<HashMap<String, ServerRoutingInfo>>>,
}

/// Routing info for a single server instance
#[allow(dead_code)] // tls_config is unused but may be needed in the future
struct ServerRoutingInfo {
    client_ports: HashSet<u16>,
    peer_ports: HashSet<u16>,
    h3_router: xlinerpc::server::StateRouter<()>,
    grpc_server: Arc<
        QuicGrpcServer<
            Command,
            CommandExecutor,
            State<Arc<CurpClient>>,
            crate::server::quic_service::XlineQuicService,
        >,
    >,
    #[allow(dead_code)]
    tls_config: TlsConfig,
}

/// Global singleton for shared QUIC state.
static SHARED_QUIC: std::sync::Mutex<Option<SharedQuicState>> = std::sync::Mutex::new(None);

/// Reset the shared QUIC state, shutting down the QuicListeners.
pub(crate) fn reset_shared_quic() {
    let mut guard = SHARED_QUIC.lock().expect("SHARED_QUIC lock poisoned");
    if let Some(state) = guard.take() {
        info!("reset_shared_quic: shutting down QuicListeners");
        state.listeners.shutdown();
    }
}

/// Xline H3 server for handling client requests via HTTP/3
pub(crate) struct XlineH3Server {
    routers: HashMap<String, (RouterBuilder, Vec<String>)>,
    client_ports: HashSet<u16>,
    peer_ports: HashSet<u16>,
}

impl XlineH3Server {
    /// Create a new Xline H3 server
    pub(crate) fn new() -> Self {
        Self {
            routers: HashMap::new(),
            client_ports: HashSet::new(),
            peer_ports: HashSet::new(),
        }
    }

    /// Set client ports
    pub(crate) fn with_client_ports(mut self, ports: HashSet<u16>) -> Self {
        self.client_ports = ports;
        self
    }

    /// Set peer ports
    pub(crate) fn with_peer_ports(mut self, ports: HashSet<u16>) -> Self {
        self.peer_ports = ports;
        self
    }

    /// Add a server router
    pub(crate) fn add_server(
        mut self,
        name: &str,
        router: RouterBuilder,
        peer_urls: impl IntoIterator<Item = String>,
    ) -> Self {
        if self.routers.contains_key(name) {
            panic!("duplicate server name: {name}");
        }
        let _ = self
            .routers
            .insert(name.to_string(), (router, peer_urls.into_iter().collect()));
        self
    }

    /// Start the server
    pub(crate) async fn serve(
        self,
        grpc_server: QuicGrpcServer<
            Command,
            CommandExecutor,
            State<Arc<CurpClient>>,
            crate::server::quic_service::XlineQuicService,
        >,
    ) -> Result<(), anyhow::Error> {
        debug!(
            client_ports = ?self.client_ports,
            peer_ports = ?self.peer_ports,
            routers_count = self.routers.len(),
            "serve start"
        );

        let is_first;
        {
            let mut guard = SHARED_QUIC.lock().unwrap();
            if guard.is_none() {
                info!("Creating QuicListeners (FIRST server)");
                let listeners = QuicListeners::builder()
                    .map(|builder| {
                        builder
                            .without_client_cert_verifier()
                            .with_parameters(gm_quic::prelude::handy::server_parameters())
                            .enable_0rtt()
                            .with_alpns(["h3"])
                            .listen(4096)
                    })
                    .map_err(|e| anyhow::anyhow!("QuicListeners::builder failed: {e}"))?;

                let shared = SharedQuicState {
                    listeners,
                    servers: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
                };
                *guard = Some(shared);
                is_first = true;
            } else {
                is_first = false;
            }
        }

        let (listeners, servers_map) = {
            let guard = SHARED_QUIC.lock().unwrap();
            let shared = guard.as_ref().ok_or_else(|| {
                anyhow::anyhow!("SHARED_QUIC not initialized (should not happen)")
            })?;
            (Arc::clone(&shared.listeners), Arc::clone(&shared.servers))
        };

        // Register servers with QuicListeners
        for (server_name, (router_builder, peer_urls)) in &self.routers {
            info!(server_name, peer_urls = ?peer_urls, "registering server");

            let cert_path = router_builder
                .tls_config
                .peer_cert_path
                .clone()
                .ok_or_else(|| {
                    anyhow::anyhow!("server tls cert config is needed: {server_name}")
                })?;
            let key_path = router_builder
                .tls_config
                .peer_key_path
                .clone()
                .ok_or_else(|| anyhow::anyhow!("server tls key config is needed: {server_name}"))?;

            let bind_uris: Vec<_> = peer_urls
                .iter()
                .map(|s| parse_bind_uri(s))
                .collect::<anyhow::Result<Vec<_>>>()?;
            debug!(server_name, ?bind_uris, "bind URIs parsed");

            listeners.add_server(
                server_name,
                cert_path.as_path(),
                key_path.as_path(),
                bind_uris,
                None,
            )?;
            info!(server_name, "server add_server done");

            // Register the server_name itself as an SNI alias (e.g., "server0", "server1")
            // This allows clients to connect using DNS names that match the certificate SAN
            if listeners.get_server(server_name).is_none() {
                // Use the first peer URL for the port
                let first_url = peer_urls.iter().next();
                if let Some(url_str) = first_url {
                    if let Ok(bind_uris) = vec![parse_bind_uri(url_str)]
                        .into_iter()
                        .collect::<Result<Vec<_>, _>>()
                    {
                        debug!("Registering server_name '{}' as SNI alias", server_name);
                        if let Err(e) = listeners.add_server(
                            server_name,
                            cert_path.as_path(),
                            key_path.as_path(),
                            bind_uris,
                            None,
                        ) {
                            warn!(
                                "server_name SNI alias '{}' registration failed: {}",
                                server_name, e
                            );
                        }
                    }
                }
            }

            // Also register all listen URLs (both peer and client) as virtual hosts
            // This allows clients to connect using IP addresses instead of DNS names
            // We need to register each unique host:port combination
            for url_str in peer_urls {
                if let Some(host) = extract_host_from_url(url_str) {
                    debug!(
                        "Checking SNI alias: host='{}', server_name='{}'",
                        host, server_name
                    );
                    // Always register the host:port for this URL so clients can connect
                    // The QuicListeners will route to the correct port
                    let bind_uris: Vec<_> = vec![parse_bind_uri(url_str)?];
                    debug!(
                        "Registering SNI alias '{}' for server '{}' with URL {}",
                        host, server_name, url_str
                    );
                    match listeners.add_server(
                        host,
                        cert_path.as_path(),
                        key_path.as_path(),
                        bind_uris,
                        None,
                    ) {
                        Ok(()) => {
                            info!(
                                "Registered SNI alias '{}' for server '{}'",
                                host, server_name
                            );
                        }
                        Err(e) => {
                            warn!(
                                "SNI alias '{}' registration failed (non-fatal): {}",
                                host, e
                            );
                        }
                    }
                }
            }

            let bind_map = listeners
                .get_server(server_name)
                .ok_or_else(|| {
                    anyhow::anyhow!("server {} not found after registration", server_name)
                })?
                .bind_interfaces();
            debug!(
                server_name,
                bind_count = bind_map.len(),
                "bind interfaces after add_server"
            );

            let (_, state) = bind_map
                .iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("server {} has no bound interfaces", server_name))?;
            let _interface = state.borrow()?;
            debug!(server_name, "server interface bind verified");
        }

        // Add routing info to shared map
        {
            let mut servers = servers_map.write().await;
            let grpc_server = Arc::new(grpc_server);

            for (server_name, (router_builder, peer_urls)) in self.routers {
                let tls_config = router_builder.tls_config.clone();
                let routing_info = ServerRoutingInfo {
                    client_ports: self.client_ports.clone(),
                    peer_ports: self.peer_ports.clone(),
                    h3_router: router_builder.into_inner(),
                    grpc_server: Arc::clone(&grpc_server),
                    tls_config: tls_config.clone(),
                };
                let existing = servers.insert(server_name.clone(), routing_info);
                if existing.is_some() {
                    warn!(server_name, "server routing info already existed, replaced");
                }
                info!(server_name, ?self.client_ports, ?self.peer_ports, "server routing info added to shared map");

                // Add SNI alias entries
                for url_str in &peer_urls {
                    if let Some(host) = extract_host_from_url(url_str) {
                        if host != server_name && !servers.contains_key(host) {
                            let alias_info = ServerRoutingInfo {
                                client_ports: self.client_ports.clone(),
                                peer_ports: self.peer_ports.clone(),
                                h3_router: servers.get(&server_name).unwrap().h3_router.clone(),
                                grpc_server: Arc::clone(&grpc_server),
                                tls_config: tls_config.clone(),
                            };
                            let _ = servers.insert(host.to_string(), alias_info);
                            debug!(
                                alias = host,
                                server = server_name,
                                "SNI routing alias added"
                            );
                        }
                    }
                }
            }
        }

        if is_first {
            info!("starting global accept loop (first server)");

            loop {
                trace!("waiting for incoming connection...");
                let (new_conn, server_name, pathway, _link) = listeners
                    .accept()
                    .await
                    .map_err(|e| anyhow::anyhow!("quic listener accept failed: {e}"))?;

                debug!(server_name, pathway = ?pathway.local(), "connection accepted");

                if let EndpointAddr::Socket(socket_addr) = pathway.local() {
                    let local_port = socket_addr.addr().port();
                    trace!(server_name, local_port, "accepted connection");

                    let servers = servers_map.read().await;
                    if let Some(routing) = servers.get(&server_name) {
                        trace!(
                            server_name,
                            local_port,
                            is_client_port = routing.client_ports.contains(&local_port),
                            is_peer_port = routing.peer_ports.contains(&local_port),
                            "checking port against routing info"
                        );

                        if routing.peer_ports.contains(&local_port) {
                            debug!(server_name, local_port, "dispatching to peer grpc server");
                            let _handle = routing.grpc_server.spawn_connection(new_conn);
                        } else if routing.client_ports.contains(&local_port) {
                            debug!(server_name, local_port, "dispatching to client h3 router");
                            let h3_conn = match h3::server::Connection::new(
                                h3_shim::QuicConnection::new(Arc::new(new_conn)),
                            )
                            .await
                            {
                                Ok(h3_conn) => {
                                    debug!(
                                        "Accept a new quic connection on peer port {local_port}"
                                    );
                                    h3_conn
                                }
                                Err(error) => {
                                    error!(local_port, error = %error, "failed to establish h3 connection");
                                    continue;
                                }
                            };
                            let router = routing.h3_router.clone();
                            let _ = tokio::spawn(Self::handle_connection(router, h3_conn));
                        } else {
                            error!(
                                local_port,
                                ?routing.client_ports,
                                ?routing.peer_ports,
                                "received connection on unknown local port"
                            );
                            continue;
                        }
                    } else {
                        warn!(
                            server_name,
                            "server not found in routing table, skipping connection"
                        );
                        continue;
                    }
                } else {
                    warn!(pathway = ?pathway.local(), "could not get local port from pathway");
                }
            }
        } else {
            info!("not first server - registration done, accept loop already running");
            Ok(())
        }
    }

    async fn handle_connection<T>(
        router: xlinerpc::server::StateRouter<()>,
        mut connection: h3::server::Connection<T, Bytes>,
    ) where
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
                            error!("Handling request failed: {}", e);
                            e
                        })
                    });
                }
                Ok(None) => {
                    trace!("connection accepted, no pending requests");
                    break;
                }
                Err(e) => {
                    error!("encounter an error: {e:?}");
                    break;
                }
            }
        }
    }
}

async fn unimplemented() -> impl axum::response::IntoResponse {
    error!("unimplemented");
    let status = http::StatusCode::OK;
    let grpc_unimplemented_code = i32::from(xlinerpc::Code::Unimplemented).to_string();
    let headers = [
        (
            http::header::HeaderName::from_static("grpc-status"),
            http::HeaderValue::from_str(&grpc_unimplemented_code)
                .unwrap_or_else(|_| http::HeaderValue::from_static("12")),
        ),
        (
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/grpc"),
        ),
    ];
    (status, headers)
}

async fn handle_request<T, SVC, ResBody>(
    request: Request<()>,
    stream: RequestStream<T, Bytes>,
    mut service: SVC,
) -> Result<(), anyhow::Error>
where
    T: BidiStream<Bytes> + 'static,
    SVC: Service<
            Request<xlinerpc::server::QuicIncomingBody<T::RecvStream>>,
            Response = Response<ResBody>,
        > + Clone
        + Send
        + 'static,
    SVC::Future: Send + 'static,
    SVC::Error: Into<anyhow::Error> + Send + Sync + std::error::Error,
    ResBody: http_body::Body<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<anyhow::Error> + Send + Sync + std::error::Error + 'static,
{
    let (mut send, recv) = stream.split();
    let body = xlinerpc::server::QuicIncomingBody::new(
        recv,
        request
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .and_then(|len| len.to_str().ok().and_then(|x| x.parse().ok())),
    );
    // Wait for the service to be ready before processing the request
    // This is required by the Tower Service contract
    let _ = service.ready().await.map_err(|e| {
        error!("service not ready: {}", e);
        anyhow::anyhow!("service not ready: {}", e)
    })?;
    let (parts, _) = request.into_parts();
    let resp = service.call(Request::from_parts(parts, body)).await?;
    let (parts, body) = resp.into_parts();
    send.send_response(Response::from_parts(parts, ())).await?;
    copy_response_body(send, body).await?;
    Ok(())
}

async fn copy_response_body<S, ResBody>(
    mut send: RequestStream<S, Bytes>,
    body: ResBody,
) -> Result<(), anyhow::Error>
where
    S: SendStream<Bytes>,
    ResBody: http_body::Body<Data = Bytes>,
    ResBody::Error: Into<anyhow::Error> + Send + Sync + std::error::Error + 'static,
{
    let mut body = std::pin::pin!(body);

    while let Some(frame) = futures::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await {
        match frame?.into_data() {
            Ok(data) => send.send_data(data).await?,
            Err(frame) => {
                if let Ok(trailers) = frame.into_trailers() {
                    send.send_trailers(trailers).await?;
                } else {
                    warn!("failed to get body frame");
                }
                continue;
            }
        }
    }

    send.finish().await?;

    Ok(())
}
