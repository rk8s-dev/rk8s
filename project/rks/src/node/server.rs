use crate::node::Shared;
use crate::node::cert::build_quic_config;
use crate::node::dispatch::{dispatch_user, dispatch_worker};
use crate::node::register::NodeRegister;
use crate::node::server::private::Sealed;
use crate::node::watcher::PodsWatcher;
use crate::protocol::config::config_ref;
use crate::vault::{CertRole, Vault};
use async_trait::async_trait;
use common::quic::RksConnection;
use common::{RksMessage, log_error, reply_and_bail};
use humantime::format_rfc3339;
use libvault::modules::pki::CertExt;
use log::{debug, error, info};
use quinn::{Connection, Endpoint};
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub struct QUICServer {
    endpoint: Arc<Endpoint>,
    context: Arc<RotateContext>,
}

struct RotateContext {
    vault: Option<Arc<Vault>>,
    deadline: Option<SystemTime>,
}

impl QUICServer {
    pub async fn new(addr: SocketAddr, vault: Option<Arc<Vault>>) -> anyhow::Result<Self> {
        // Create QUIC endpoint and server certificate
        let (config, certs) = build_quic_config(vault.as_deref()).await?;

        let deadline = match certs {
            Some(certs) => Some(certs[0].rotate_deadline(2.0 / 3.0)?),
            None => None,
        };

        Ok(Self {
            endpoint: Arc::new(Endpoint::server(config, addr)?),
            context: Arc::new(RotateContext { vault, deadline }),
        })
    }

    fn rotate_background(&self) {
        if let (Some(deadline), Some(vault)) = (self.context.deadline, self.context.vault.clone()) {
            info!("next rotation deadline: {}", format_rfc3339(deadline));

            let endpoint = self.endpoint.clone();

            tokio::spawn(async move {
                let result =
                    || -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'static>> {
                        let mut deadline = deadline;
                        let vault = vault.clone();

                        Box::pin(async move {
                            loop {
                                let now = SystemTime::now();
                                let duration = deadline.duration_since(now).unwrap_or_else(|err| {
                                    info!(
                                        "rotation deadline already passed by {:?}, rotating immediately",
                                        err.duration()
                                    );
                                    Duration::ZERO
                                });

                                let until = tokio::time::Instant::now() + duration;
                                tokio::time::sleep_until(until).await;

                                info!("deadline reached, preparing rotate certificate");

                                let (config, certs) =
                                    build_quic_config(Some(vault.as_ref())).await?;

                                let certs = certs.unwrap();
                                deadline = certs[0].rotate_deadline(2.0 / 3.0)?;
                                info!("next rotation deadline: {}", format_rfc3339(deadline));

                                endpoint.set_server_config(Some(config))
                            }
                        })
                    };

                log_error!(result().await)
            });
        }
    }

    /// Launch the RKS server to listen for incoming QUIC connections.
    /// Each connection will be handled in a dedicated task.
    pub async fn serve(&self, shared: Arc<Shared>) -> anyhow::Result<()> {
        // Accept loop
        loop {
            self.rotate_background();

            let incoming = match self.endpoint.accept().await {
                Some(incoming) => incoming,
                None => anyhow::bail!("The endpoint is closed unexpectedly"),
            };

            let shared = shared.clone();
            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => {
                        let cfg = config_ref();
                        let result = if !cfg.tls_config.enable
                            || connection.peer_identity().is_some()
                        {
                            let conn = AuthConnection::<Verified>::new(connection, shared.clone());
                            conn.serve().await
                        } else {
                            let conn =
                                AuthConnection::<Unauthenticated>::new(connection, shared.clone());
                            conn.serve().await
                        };
                        if let Err(e) = result {
                            let msg = e.to_string();
                            // "closed by peer: 0" means normal QUIC NO_ERROR close
                            if msg.contains("closed by peer: 0") {
                                debug!("connection closed normally");
                            } else {
                                error!("{e}");
                            }
                        }
                    }
                    Err(e) => error!("Connection failed: {e}"),
                }
            });
        }
    }
}

mod private {
    pub trait Sealed {}
}

pub struct Unauthenticated;

impl Sealed for Unauthenticated {}

pub struct Verified;

impl Sealed for Verified {}

#[async_trait]
pub trait ConnectionState: Sealed {
    async fn serve(conn: AuthConnection<Self>) -> anyhow::Result<()>
    where
        Self: Sized;
}

/// Typed connection wrapper that tracks authentication state at compile time.
pub struct AuthConnection<State> {
    conn: RksConnection,
    shared: Arc<Shared>,
    // The state type parameter (Verified/Unauthenticated) forces callers to only
    // use APIs allowed for that phase of the handshake without runtime checks.
    state: PhantomData<State>,
}

impl<State: Sealed> AuthConnection<State> {
    pub fn new(conn: Connection, shared: Arc<Shared>) -> AuthConnection<State> {
        Self {
            conn: RksConnection::new(conn),
            shared,
            state: PhantomData,
        }
    }
}

impl<State: ConnectionState> AuthConnection<State> {
    pub async fn serve(self) -> anyhow::Result<()> {
        State::serve(self).await
    }
}

impl AuthConnection<Verified> {
    async fn classify_connection(&self) -> anyhow::Result<(bool, Option<String>)> {
        // Initial handshake to classify connection (RegisterNode or UserRequest)
        let msg = self.conn.fetch_msg().await?;
        match &msg {
            RksMessage::RegisterNode(node) => {
                let register = NodeRegister::new(&self.conn, self.shared.as_ref());
                register.register(node.clone()).await
            }
            RksMessage::UserRequest(_req) => {
                info!("user connection established");
                Ok((false, None))
            }
            _ => reply_and_bail!(
                self.conn,
                &msg,
                RksMessage::RegisterNode { .. } | RksMessage::UserRequest { .. }
            ),
        }
    }

    async fn dispatch_loop(&self, is_worker: bool) -> anyhow::Result<()> {
        // Main loop: accept application messages for ongoing communication
        loop {
            let msg = self.conn.fetch_msg().await?;
            info!("fetched message: {msg}");

            if is_worker {
                log_error!(dispatch_worker(msg, &self.conn, &self.shared.xline_store).await);
                continue;
            }

            log_error!(dispatch_user(msg, &self.conn, &self.shared.xline_store).await)
        }
    }
}

#[async_trait]
impl ConnectionState for Unauthenticated {
    async fn serve(conn: AuthConnection<Self>) -> anyhow::Result<()> {
        debug!("waiting for auth request from client");

        let msg = conn.conn.fetch_msg().await?;

        debug!("received request from client");
        match &msg {
            RksMessage::CertificateSign { req, token } => {
                debug!("return issued certificate to client");

                let reply = match conn.shared.vault.as_ref() {
                    Some(vault) => match vault.validate_token(token).await {
                        Ok(_) => {
                            let res = vault.issue_cert(CertRole::Rkl, req).await?;
                            RksMessage::Certificate(res)
                        }
                        Err(e) => RksMessage::Error(format!("Invalid join token: {e}")),
                    },
                    None => RksMessage::Error(
                        "TLS authentication is disabled on the server".to_string(),
                    ),
                };

                conn.conn.send_msg(&reply).await?;

                debug!("waiting for client to close auth connection");
                let _ = conn.conn.closed().await;
                debug!("auth connection closed by client");
            }
            _ => reply_and_bail!(conn.conn, &msg, RksMessage::CertificateSign { .. }),
        }
        Ok(())
    }
}

#[async_trait]
impl ConnectionState for Verified {
    /// Handle an individual connection (worker or user).
    /// Classifies client type and spawns watchers for workers.
    async fn serve(conn: AuthConnection<Self>) -> anyhow::Result<()> {
        let (is_worker, node_id) = conn.classify_connection().await?;

        if is_worker && let Some(node_id) = node_id {
            // Start watching pods if this is a registered worker node
            let watcher = PodsWatcher::new(node_id, conn.conn.clone(), conn.shared.clone());
            watcher.spawn()?;
        }

        conn.dispatch_loop(is_worker).await
    }
}
