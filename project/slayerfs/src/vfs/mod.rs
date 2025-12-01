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
pub mod cache;
pub mod demo;
pub mod fs;
pub mod handles;
pub mod inode;
pub mod io;
pub mod sdk;
pub mod simple;

// Module implementation TODOs remain.

const CHUNK_ID_BASE: u64 = 1_000_000_000u64;

pub fn chunk_id_for(ino: i64, chunk_index: u64) -> u64 {
    let ino_u64 = u64::try_from(ino).expect("inode must be non-negative");
    ino_u64
        .checked_mul(CHUNK_ID_BASE)
        .and_then(|v| v.checked_add(chunk_index))
        .unwrap_or_else(|| {
            panic!(
                "chunk_id overflow for inode {} chunk_index {}",
                ino, chunk_index
            )
        })
}

/// Extracts the inode number and chunk index from a chunk_id.
/// This is the inverse operation of `chunk_id_for`.
///
/// # Returns
/// A tuple of (inode, chunk_index) where:
/// - inode = chunk_id / CHUNK_ID_BASE
/// - chunk_index = chunk_id % CHUNK_ID_BASE
pub fn extract_ino_and_chunk_index(chunk_id: u64) -> (i64, u64) {
    let ino = (chunk_id / CHUNK_ID_BASE) as i64;
    let chunk_index = chunk_id % CHUNK_ID_BASE;
    (ino, chunk_index)
}
