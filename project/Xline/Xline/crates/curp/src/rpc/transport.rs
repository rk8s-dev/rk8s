//! Transport configuration for RPC layer

use std::sync::Arc;

use dquic::prelude::QuicClient;

/// Transport layer configuration
///
/// Holds QUIC transport settings for RPC connections.
pub struct TransportConfig {
    /// QUIC client
    pub client: Arc<QuicClient>,
    /// DNS fallback mode
    pub dns_fallback: super::quic_transport::channel::DnsFallback,
}

impl std::fmt::Debug for TransportConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TransportConfig::Quic(..)")
    }
}

impl Clone for TransportConfig {
    fn clone(&self) -> Self {
        Self {
            client: Arc::clone(&self.client),
            dns_fallback: self.dns_fallback,
        }
    }
}
