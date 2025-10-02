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
//! Submodules:
//! - `client`: transactional metadata client (SQLx wrappers)
//! - `migrations`: DB migration helpers
//! - `database_store`: Database-based metadata store (SQLite/PostgreSQL)
//! - `etcd_store`: Etcd-based metadata store (Xline/etcd)
//! - `factory`: Factory for creating appropriate MetaStore implementations
pub mod client;
pub mod config;
pub mod database_store;
pub mod entities;
pub mod factory;
pub mod migrations;
pub mod permission;
pub mod store;
pub mod xline_store;

// Primary exports
pub use factory::create_meta_store_from_url;
pub use permission::Permission;
pub use store::MetaStore;
