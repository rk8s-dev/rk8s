//! VFS layer (virtual filesystem)
//!
//! Responsibilities:
//! - Implement POSIX semantics, manage file handles, caching, and translation
//!   between FUSE requests and the data/meta layers.
//! - Provide read/write buffering, consistency helpers and oplock-like behavior
//!   if needed.
//! - Coordinate with the meta client for metadata lookup and the chunk writer
//!   for producing blocks.
//!
//! Important notes / TODOs:
//! - Implement handle lifecycle and cache invalidation policies.
//! - Offer concurrency-safe APIs for reader/writer paths.
//!
//! Submodules:
//! - `handles`: file and directory handle management
//! - `cache`: caching helpers and policies
pub(crate) mod backend;
pub(crate) mod cache;
pub(crate) mod config;
pub mod error;
pub mod fs;
pub(crate) mod handles;
pub(crate) mod inode;
pub(crate) mod io;
pub mod sdk;
// Module implementation TODOs remain.

pub use crate::vfs::io::{chunk_id_for, extract_ino_and_chunk_index};
pub(crate) use inode::Inode;
