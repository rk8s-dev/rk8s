//! Metadata client and schema
//!
//! Responsibilities:
//! - Provide a transactional metadata client that talks to the chosen SQL
//!   backend (Postgres for production, SQLite for single-node development) via SQLx.
//! - Expose safe, atomic operations for inode/chunk/slice/block lifecycle updates.
//! - Maintain session registration and heartbeat records used for crash recovery
//!   and cleanup.
//!
//! Important notes / TODOs:
//! - Implement DB migrations and schema versioning.
//! - Ensure critical write-path updates (blocks + slice_blocks + slices + inode.size)
//!   are committed atomically.
//!
pub mod client;
pub mod config;
pub mod entities;
pub mod factory;
pub mod layer;
pub mod migrations;
pub mod permission;
pub mod store;
pub mod stores;

// Primary exports
#[allow(dead_code)]
pub type MetaHandle<M> = factory::MetaHandle<M>;
pub use layer::MetaLayer;
pub use permission::Permission;
pub use store::MetaStore;

pub const INODE_ID_KEY: &str = "slayerfs:next_inode_id";

pub const SLICE_ID_KEY: &str = "slayerfs:next_slice_id";
