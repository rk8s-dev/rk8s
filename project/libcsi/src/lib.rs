//! # libcsi — Simplified CSI over QUIC for RK8s
//!
//! `libcsi` implements a lightweight [Container Storage Interface][csi] layer
//! that uses QUIC (via [`quinn`]) instead of gRPC for transport.  It is
//! designed to integrate with **SlayerFS** as the primary storage backend and
//! follows the RK8s architecture conventions (Tokio async runtime, `tracing`
//! for observability, `thiserror` for structured errors).
//!
//! ## Module overview
//!
//! | Module | Purpose |
//! |---|---|
//! | [`types`] | Core data model: `Volume`, `VolumeId`, capabilities, requests. |
//! | [`error`] | [`CsiError`] enum covering all failure modes. |
//! | [`message`] | [`CsiMessage`] protocol envelope for QUIC transport. |
//! | [`identity`] | [`CsiIdentity`] trait — plugin discovery & health. |
//! | [`controller`] | [`CsiController`] trait — volume create/delete. |
//! | [`node`] | [`CsiNode`] trait — stage, publish, unpublish, unstage. |
//! | [`transport`] | QUIC client/server built on `quinn`. |
//! | [`backend`] | Pluggable storage backends (SlayerFS). |
//!
//! [csi]: https://github.com/container-storage-interface/spec

pub mod backend;
pub mod controller;
pub mod error;
pub mod identity;
pub mod message;
pub mod node;
pub mod transport;
pub mod types;

// Re-export the most commonly used items at crate root for convenience.
pub use controller::CsiController;
pub use error::CsiError;
pub use identity::CsiIdentity;
pub use message::CsiMessage;
pub use node::CsiNode;
pub use types::*;
