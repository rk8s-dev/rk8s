//! Transport configuration for RPC layer
//!
//! This module defines `TransportConfig` enum for selecting between tonic gRPC
//! and QUIC transport. It is placed in a separate file to isolate `Debug`/`Clone`/`Default`
//! implementations from `QuicClient` which may not implement these traits.

#[cfg(feature = "quic")]
use std::sync::Arc;

#[cfg(feature = "quic")]
use gm_quic::prelude::QuicClient;

/// Transport layer configuration
///
/// Determines whether RPC uses tonic gRPC or QUIC transport.
#[allow(dead_code)] // Will be used in later implementation steps
#[non_exhaustive]
pub(crate) enum TransportConfig {
    /// Use tonic gRPC (default, current behavior)
    Tonic,
    /// Use QUIC transport (requires gm-quic QuicClient)
    #[cfg(feature = "quic")]
    Quic(Arc<QuicClient>, super::quic_transport::channel::DnsFallback),
}

impl std::fmt::Debug for TransportConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Tonic => write!(f, "TransportConfig::Tonic"),
            #[cfg(feature = "quic")]
            Self::Quic(..) => write!(f, "TransportConfig::Quic(..)"),
        }
    }
}

impl Clone for TransportConfig {
    fn clone(&self) -> Self {
        match *self {
            Self::Tonic => Self::Tonic,
            #[cfg(feature = "quic")]
            Self::Quic(ref c, fallback) => Self::Quic(Arc::clone(c), fallback),
        }
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::Tonic
    }
}
