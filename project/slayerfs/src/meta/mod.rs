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
pub mod client;
pub mod migrations;

// Module implementation TODOs remain.