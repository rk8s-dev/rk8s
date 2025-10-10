use crate::node::Shared;
use crate::node::dispatch::{dispatch_user, dispatch_worker};
use crate::node::register::NodeRegister;
use crate::node::server::private::Sealed;
use crate::node::watcher::PodsWatcher;
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
            let incoming = self.endpoint.accept().await;

            let shared = shared.clone();
            tokio::spawn(async move {
                if let Some(connection) = incoming {
                    match connection.await {
                        Ok(connection) => {
                            let has_identity = connection.peer_identity().is_some();
                            if has_identity {
                                let conn =
                                    AuthConnection::<Verified>::new(connection, shared.clone());
                                log_error!(conn.serve().await);
                            } else {
                                let conn = AuthConnection::<Unauthenticated>::new(
                                    connection,
                                    shared.clone(),
                                );
                                log_error!(conn.auth().await);
                            }
                        }
                        Err(e) => error!("Connection failed: {e}"),
                    }
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

impl AuthConnection<Unauthenticated> {
    pub async fn auth(self) -> anyhow::Result<()> {
        debug!("[server] waiting for auth request from client");

        let msg = self.conn.fetch_msg().await?;

        debug!("[server] received request from client");

        match &msg {
            RksMessage::CertificateSign { req, .. } => {
                let res = self.shared.vault.issue_cert("rkl", req).await?;

                debug!("[server] return issued certificate to client");
                self.conn.send_msg(&RksMessage::Certificate(res)).await?;
                debug!("[server] waiting for client to close auth connection");
                let _ = self.conn.closed().await;
                debug!("[server] auth connection closed by client");
            }
            _ => reply_and_bail!(self.conn, &msg, RksMessage::CertificateSign { .. }),
        }
        Ok(())
    }
}

impl AuthConnection<Verified> {
    pub async fn serve(self) -> anyhow::Result<()> {
        let (is_worker, node_id) = self.classify_connection().await?;

        if is_worker && let Some(node_id) = node_id {
            let watcher = PodsWatcher::new(node_id, self.conn.clone(), self.shared.clone());
            watcher.spawn()?;
        }

        self.dispatch_loop(is_worker).await?;

        Ok(())
    }

    async fn classify_connection(&self) -> anyhow::Result<(bool, Option<String>)> {
        let msg = self.conn.fetch_msg().await?;
        match &msg {
            RksMessage::RegisterNode(node) => {
                let register = NodeRegister::new(&self.conn, self.shared.clone());
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
