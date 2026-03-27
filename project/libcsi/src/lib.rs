//! # libcsi — CSI interface definitions for RK8s
//!
//! `libcsi` provides the core trait definitions, types, error types, and
//! protocol messages for a simplified [Container Storage Interface][csi]
//! within the RK8s orchestration system.
//!
//! This crate is a **pure interface crate** — it contains no transport or
//! storage backend implementations.  Concrete implementations live in the
//! RKS (Controller side) and RKL (Node side) crates which depend on
//! `libcsi` for the shared contract.
//!
//! ## Module overview
//!
//! | Module | Purpose |
//! |---|---|
//! | [`types`] | Core data model: `Volume`, `VolumeId`, capabilities, requests. |
//! | [`error`] | [`CsiError`] enum covering all failure modes. |
//! | [`message`] | [`CsiMessage`] protocol envelope. |
//! | [`identity`] | [`CsiIdentity`] trait — plugin discovery & health. |
//! | [`controller`] | [`CsiController`] trait — volume create/delete. |
//! | [`node`] | [`CsiNode`] trait — stage, publish, unpublish, unstage. |
//!
//! [csi]: https://github.com/container-storage-interface/spec

pub mod controller;
pub mod error;
pub mod identity;
pub mod message;
pub mod node;
pub mod types;

// Re-export the most commonly used items at crate root for convenience.
pub use controller::CsiController;
pub use error::CsiError;
pub use identity::CsiIdentity;
pub use message::CsiMessage;
pub use node::CsiNode;
pub use types::*;
