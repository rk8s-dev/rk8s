use crate::node::Shared;
use crate::node::dispatch::{dispatch_user, dispatch_worker};
use crate::node::register::NodeRegister;
use crate::node::server::private::Sealed;
use crate::node::watcher::PodsWatcher;
use async_trait::async_trait;
use common::quic::RksConnection;
use common::{RksMessage, log_error, reply_and_bail};
use log::{debug, error, info};
use quinn::{Connection, Endpoint, ServerConfig};
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct QUICServer {
    endpoint: Endpoint,
}

impl QUICServer {
    pub fn new(addr: SocketAddr, config: ServerConfig) -> anyhow::Result<Self> {
        Ok(Self {
            endpoint: Endpoint::server(config, addr)?,
        })
    }

    pub async fn serve(&self, shared: Arc<Shared>) -> anyhow::Result<()> {
        loop {
            let incoming = match self.endpoint.accept().await {
                Some(incoming) => incoming,
                None => anyhow::bail!("The endpoint is closed unexpectedly"),
            };

            let shared = shared.clone();
            tokio::spawn(async move {
                match incoming.await {
                    Ok(connection) => {
                        let result = if connection.peer_identity().is_some() {
                            let conn = AuthConnection::<Verified>::new(connection, shared.clone());
                            conn.serve().await
                        } else {
                            let conn =
                                AuthConnection::<Unauthenticated>::new(connection, shared.clone());
                            conn.serve().await
                        };
                        log_error!(result)
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

pub struct AuthConnection<State> {
    conn: RksConnection,
    shared: Arc<Shared>,
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
        let msg = self.conn.fetch_msg().await?;
        match &msg {
            RksMessage::RegisterNode(node) => {
                let register = NodeRegister::new(&self.conn, self.shared.as_ref());
                register.register(node.clone()).await
            }
            RksMessage::UserRequest(_req) => {
                info!("[server] user connection established");
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
        loop {
            let msg = self.conn.fetch_msg().await?;
            info!("[server] fetched message: {msg}");

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
        debug!("[server] waiting for auth request from client");

        let msg = conn.conn.fetch_msg().await?;

        debug!("[server] received request from client");

        match &msg {
            RksMessage::CertificateSign { req, .. } => {
                let res = conn.shared.vault.issue_cert("rkl", req).await?;

                debug!("[server] return issued certificate to client");
                conn.conn.send_msg(&RksMessage::Certificate(res)).await?;
                debug!("[server] waiting for client to close auth connection");
                let _ = conn.conn.closed().await;
                debug!("[server] auth connection closed by client");
            }
            _ => reply_and_bail!(conn.conn, &msg, RksMessage::CertificateSign { .. }),
        }
        Ok(())
    }
}

#[async_trait]
impl ConnectionState for Verified {
    async fn serve(conn: AuthConnection<Self>) -> anyhow::Result<()> {
        let (is_worker, node_id) = conn.classify_connection().await?;

        if is_worker && let Some(node_id) = node_id {
            let watcher = PodsWatcher::new(node_id, conn.conn.clone(), conn.shared.clone());
            watcher.spawn()?;
        }

        conn.dispatch_loop(is_worker).await
    }
}
