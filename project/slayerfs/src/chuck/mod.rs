//! Chunk and slice management (chuck)
//!
//! Responsibilities:
//! - Manage logical chunks and slices that compose file data. A chunk is a
//!   larger logical aggregation (e.g., 64 MiB) composed of smaller blocks.
//! - Provide operations to create slices, map slice offsets to block boundaries,
//!   and maintain lifecycle state for slices (pending, committed).
//! - Expose utilities for chunk/slice compaction and splitting.
//!
//! Important notes / TODOs:
//! - Implement chunk indexing and fast lookup based on inode + chunk_index.
//! - Provide helpers to convert write buffers into slice -> blocks for the writer.
//!
//! Submodules:
//! - `chunk`: chunk index and metadata helpers
//! - `slice`: slice lifecycle and mapping to blocks
pub mod chunk;
pub mod slice;

// Module implementation TODOs remain.
