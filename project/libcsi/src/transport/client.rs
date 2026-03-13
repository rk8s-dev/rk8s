//! QUIC client used by the RKS control plane to issue CSI requests.

use std::net::SocketAddr;
use std::sync::Arc;

use quinn::crypto::rustls::QuicClientConfig;
use tracing::{debug, instrument};

use crate::error::CsiError;
use crate::message::CsiMessage;

/// A lightweight CSI client that sends [`CsiMessage`] requests over a single
/// QUIC connection and returns the server's response.
pub struct CsiClient {
    connection: quinn::Connection,
}

impl CsiClient {
    /// Establish a new QUIC connection to the CSI server at `addr`.
    ///
    /// * `addr` — socket address of the remote CSI server
    /// * `server_name` — TLS SNI name that must match a SAN in the server's
    ///   certificate (typically the hostname or a fixed name agreed upon when
    ///   certificates are issued by `libvault`)
    /// * `tls_config` — client TLS configuration, typically built from
    ///   certificates issued by `libvault`
    pub async fn connect(
        addr: SocketAddr,
        server_name: &str,
        tls_config: rustls::ClientConfig,
    ) -> Result<Self, CsiError> {
        let quic_client_config = QuicClientConfig::try_from(tls_config)
            .map_err(|e| CsiError::TransportError(format!("invalid TLS config: {e}")))?;
        let client_config = quinn::ClientConfig::new(Arc::new(quic_client_config));

        let mut endpoint =
            quinn::Endpoint::client("0.0.0.0:0".parse().unwrap()).map_err(CsiError::transport)?;
        endpoint.set_default_client_config(client_config);

        let connection = endpoint
            .connect(addr, server_name)
            .map_err(CsiError::transport)?
            .await
            .map_err(CsiError::transport)?;

        debug!(%addr, %server_name, "CSI QUIC connection established");
        Ok(Self { connection })
    }

    /// Send a request and wait for the corresponding response.
    ///
    /// Each call opens a new bi-directional QUIC stream, writes the
    /// JSON-serialized request, finishes the send side, then reads the
    /// full response and deserializes it.
    #[instrument(skip(self), fields(msg = %msg))]
    pub async fn request(&self, msg: &CsiMessage) -> Result<CsiMessage, CsiError> {
        let (mut send, mut recv) = self
            .connection
            .open_bi()
            .await
            .map_err(CsiError::transport)?;

        // Serialize and send.
        let payload = serde_json::to_vec(msg).map_err(CsiError::internal)?;
        send.write_all(&payload)
            .await
            .map_err(CsiError::transport)?;
        send.finish().map_err(CsiError::transport)?;

        // Read the full response.
        let buf = recv
            .read_to_end(16 * 1024 * 1024) // 16 MiB upper bound
            .await
            .map_err(CsiError::transport)?;

        let response: CsiMessage = serde_json::from_slice(&buf).map_err(CsiError::transport)?;
        debug!(%response, "CSI response received");
        Ok(response)
    }

    /// Close the underlying QUIC connection gracefully.
    pub fn close(&self) {
        self.connection
            .close(quinn::VarInt::from_u32(0), b"client shutdown");
    }
}
