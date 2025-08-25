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
//! - `handles`: file handle and descriptor management
//! - `cache`: caching helpers and policies
pub mod handles;
pub mod cache;

// Module implementation TODOs remain.