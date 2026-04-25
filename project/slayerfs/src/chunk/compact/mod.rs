//! Chunk compaction and garbage collection utilities.
//!
//! This module provides:
//! - `Compactor`: Coordinates MetaStore and BlockStore to compact chunk slices
//! - `CompactionWorker`: Background worker for automatic compaction
//! - `BlockStoreGC`: Garbage collector for cleaning up orphaned block data

pub mod compactor;
pub mod gc;
pub mod worker;

pub use compactor::{CompactResult, Compactor, CompactorError};
pub use gc::{BlockGcConfig, BlockStoreGC, GCError};
pub use worker::{ChunkLockGuard, CompactLockManager, CompactionWorker, CompactionWorkerConfig};
