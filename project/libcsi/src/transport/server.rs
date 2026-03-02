//! QUIC server that runs on each worker node and dispatches incoming CSI
//! requests to the appropriate trait implementations.

use std::net::SocketAddr;
use std::sync::Arc;

use quinn::crypto::rustls::QuicServerConfig;
use tracing::{debug, error, info, instrument, warn};

use crate::controller::CsiController;
use crate::error::CsiError;
use crate::identity::CsiIdentity;
use crate::message::CsiMessage;
use crate::node::CsiNode;

/// A CSI server that accepts QUIC connections and dispatches
/// [`CsiMessage`] requests to an [`CsiIdentity`] + [`CsiController`] +
/// [`CsiNode`] implementation.
pub struct CsiServer<T> {
    endpoint: quinn::Endpoint,
    handler: Arc<T>,
}

impl<T> CsiServer<T>
where
    T: CsiIdentity + CsiController + CsiNode + 'static,
{
    /// Create a new server bound to `addr`.
    ///
    /// `tls_config` is typically built from a certificate and key signed by
    /// `libvault`.
    pub fn new(
        addr: SocketAddr,
        tls_config: rustls::ServerConfig,
        handler: Arc<T>,
    ) -> Result<Self, CsiError> {
        let quic_server_config = QuicServerConfig::try_from(tls_config)
            .map_err(|e| CsiError::TransportError(format!("invalid TLS config: {e}")))?;
        let server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_server_config));
        let endpoint = quinn::Endpoint::server(server_config, addr).map_err(CsiError::transport)?;
        info!(%addr, "CSI QUIC server listening");
        Ok(Self { endpoint, handler })
    }

    /// Accept connections in a loop until the endpoint is closed.
    ///
    /// Each accepted connection spawns a Tokio task, and each bi-stream
    /// within a connection is handled concurrently.
    pub async fn serve(&self) -> Result<(), CsiError> {
        while let Some(incoming) = self.endpoint.accept().await {
            let handler = Arc::clone(&self.handler);
            tokio::spawn(async move {
                match incoming.await {
                    Ok(conn) => {
                        let remote = conn.remote_address();
                        debug!(%remote, "CSI connection accepted");
                        if let Err(e) = Self::handle_connection(conn, handler).await {
                            warn!(%remote, error = %e, "CSI connection error");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "CSI incoming connection failed");
                    }
                }
            });
        }
        Ok(())
    }

    /// Handle all bi-streams on a single connection.
    async fn handle_connection(conn: quinn::Connection, handler: Arc<T>) -> Result<(), CsiError> {
        loop {
            let (send, recv) = match conn.accept_bi().await {
                Ok(stream) => stream,
                Err(quinn::ConnectionError::ApplicationClosed(_)) => return Ok(()),
                Err(e) => return Err(CsiError::transport(e)),
            };

            let handler = Arc::clone(&handler);
            tokio::spawn(async move {
                if let Err(e) = Self::handle_stream(send, recv, &handler).await {
                    error!(error = %e, "CSI stream handler error");
                }
            });
        }
    }

    /// Process a single bi-stream: read request → dispatch → write response.
    #[instrument(skip_all)]
    async fn handle_stream(
        mut send: quinn::SendStream,
        mut recv: quinn::RecvStream,
        handler: &T,
    ) -> Result<(), CsiError> {
        // Read the full request.
        let buf = recv
            .read_to_end(16 * 1024 * 1024)
            .await
            .map_err(CsiError::transport)?;

        let request: CsiMessage = serde_json::from_slice(&buf)
            .map_err(|e| CsiError::TransportError(format!("malformed request: {e}")))?;

        debug!(%request, "CSI request received");

        let response = Self::dispatch(handler, request).await;

        // Serialize and send the response.
        let payload = serde_json::to_vec(&response).map_err(CsiError::internal)?;
        send.write_all(&payload)
            .await
            .map_err(CsiError::transport)?;
        send.finish().map_err(CsiError::transport)?;
        Ok(())
    }

    /// Map a [`CsiMessage`] request to the correct trait method call and
    /// wrap the result in a response [`CsiMessage`].
    async fn dispatch(handler: &T, request: CsiMessage) -> CsiMessage {
        match request {
            // --- Identity ---------------------------------------------------
            CsiMessage::Probe => match handler.probe().await {
                Ok(ok) => CsiMessage::ProbeResult(ok),
                Err(e) => CsiMessage::Error(e),
            },
            CsiMessage::GetPluginInfo => match handler.get_plugin_info().await {
                Ok(info) => CsiMessage::PluginInfoResponse(info),
                Err(e) => CsiMessage::Error(e),
            },
            CsiMessage::GetPluginCapabilities => match handler.get_plugin_capabilities().await {
                Ok(caps) => CsiMessage::PluginCapabilitiesResponse(caps),
                Err(e) => CsiMessage::Error(e),
            },

            // --- Controller -------------------------------------------------
            CsiMessage::CreateVolume(req) => match handler.create_volume(req).await {
                Ok(vol) => CsiMessage::VolumeCreated(vol),
                Err(e) => CsiMessage::Error(e),
            },
            CsiMessage::DeleteVolume(id) => match handler.delete_volume(&id).await {
                Ok(()) => CsiMessage::Ok,
                Err(e) => CsiMessage::Error(e),
            },
            CsiMessage::ListVolumes => match handler.list_volumes().await {
                Ok(vols) => CsiMessage::VolumeList(vols),
                Err(e) => CsiMessage::Error(e),
            },
            CsiMessage::GetCapacity => match handler.get_capacity().await {
                Ok(cap) => CsiMessage::Capacity(cap),
                Err(e) => CsiMessage::Error(e),
            },
            CsiMessage::ValidateVolumeCapabilities {
                volume_id,
                capabilities,
            } => match handler
                .validate_volume_capabilities(&volume_id, &capabilities)
                .await
            {
                Ok(valid) => CsiMessage::CapabilitiesValid(valid),
                Err(e) => CsiMessage::Error(e),
            },

            // --- Node -------------------------------------------------------
            CsiMessage::StageVolume(req) => match handler.stage_volume(req).await {
                Ok(()) => CsiMessage::Ok,
                Err(e) => CsiMessage::Error(e),
            },
            CsiMessage::UnstageVolume {
                volume_id,
                staging_target_path,
            } => match handler
                .unstage_volume(&volume_id, &staging_target_path)
                .await
            {
                Ok(()) => CsiMessage::Ok,
                Err(e) => CsiMessage::Error(e),
            },
            CsiMessage::PublishVolume(req) => match handler.publish_volume(req).await {
                Ok(()) => CsiMessage::Ok,
                Err(e) => CsiMessage::Error(e),
            },
            CsiMessage::UnpublishVolume {
                volume_id,
                target_path,
            } => match handler.unpublish_volume(&volume_id, &target_path).await {
                Ok(()) => CsiMessage::Ok,
                Err(e) => CsiMessage::Error(e),
            },
            CsiMessage::GetNodeInfo => match handler.get_info().await {
                Ok(info) => CsiMessage::NodeInfoResponse(info),
                Err(e) => CsiMessage::Error(e),
            },

            // --- Response variants should never arrive as requests ----------
            other => {
                warn!(msg = %other, "unexpected message variant received as request");
                CsiMessage::Error(CsiError::InvalidArgument(format!(
                    "unexpected message: {other}"
                )))
            }
        }
    }

    /// Return a reference to the underlying QUIC endpoint, useful for
    /// obtaining the local address or shutting down.
    pub fn endpoint(&self) -> &quinn::Endpoint {
        &self.endpoint
    }
}
