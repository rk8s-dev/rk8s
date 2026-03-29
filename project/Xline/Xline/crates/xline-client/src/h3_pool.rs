//! H3 connection pool - holds client reference for creating new connections

use std::sync::Arc;

use gm_quic::prelude::QuicClient;

/// Pool just holds the QuicClient reference
pub(crate) struct H3ConnectionPool {
    client: Arc<QuicClient>,
}

impl H3ConnectionPool {
    /// Create new connection pool
    pub(crate) fn new(client: Arc<QuicClient>) -> Self {
        Self { client }
    }

    /// Get client reference
    pub(crate) fn client(&self) -> Arc<QuicClient> {
        Arc::clone(&self.client)
    }
}
