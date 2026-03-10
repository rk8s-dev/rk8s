//! CSI error types.
//!
//! All errors in the `libcsi` crate are represented by the [`CsiError`] enum,
//! which derives [`thiserror::Error`] for ergonomic error handling and also
//! implements [`Serialize`]/[`Deserialize`] so errors can travel across the
//! QUIC transport layer.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Unified error type for CSI operations.
#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum CsiError {
    /// The requested volume already exists.
    #[error("volume {0} already exists")]
    VolumeAlreadyExists(String),

    /// The requested volume was not found.
    #[error("volume {0} not found")]
    VolumeNotFound(String),

    /// A mount operation failed.
    #[error("mount failed at {path}: {reason}")]
    MountFailed {
        /// Filesystem path where the mount was attempted.
        path: String,
        /// Human-readable failure reason.
        reason: String,
    },

    /// An unmount operation failed.
    #[error("unmount failed at {path}: {reason}")]
    UnmountFailed {
        /// Filesystem path where the unmount was attempted.
        path: String,
        /// Human-readable failure reason.
        reason: String,
    },

    /// The storage backend (e.g. SlayerFS) returned an error.
    #[error("backend error: {0}")]
    BackendError(String),

    /// A QUIC / transport-level error.
    #[error("transport error: {0}")]
    TransportError(String),

    /// The caller supplied an invalid argument.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// An unclassified internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl CsiError {
    /// Create a [`CsiError::BackendError`] from anything that implements
    /// [`std::fmt::Display`].
    pub fn backend<E: std::fmt::Display>(e: E) -> Self {
        Self::BackendError(e.to_string())
    }

    /// Create a [`CsiError::TransportError`] from anything that implements
    /// [`std::fmt::Display`].
    pub fn transport<E: std::fmt::Display>(e: E) -> Self {
        Self::TransportError(e.to_string())
    }

    /// Create a [`CsiError::Internal`] from anything that implements
    /// [`std::fmt::Display`].
    pub fn internal<E: std::fmt::Display>(e: E) -> Self {
        Self::Internal(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = CsiError::VolumeNotFound("vol-123".into());
        assert_eq!(err.to_string(), "volume vol-123 not found");
    }

    #[test]
    fn error_serde_roundtrip() {
        let err = CsiError::MountFailed {
            path: "/mnt/test".into(),
            reason: "permission denied".into(),
        };
        let json = serde_json::to_string(&err).expect("serialize");
        let de: CsiError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(err.to_string(), de.to_string());
    }
}
