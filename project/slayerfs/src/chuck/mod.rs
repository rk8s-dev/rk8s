//! Chunk/slice management utilities.
//!
//! Design goals (JuiceFS-inspired):
//! - Files are cut into fixed-size chunks (e.g., 64 MiB) and each chunk is subdivided into fixed-size blocks (e.g., 4 MiB).
//! - Writes produce slices (contiguous ranges inside a chunk) made of block spans; metadata records the slice→block mapping.
//! - Reads locate slices via `(ino, chunk_index)` and stitch block spans by offset.
//!
//! This module exposes:
//! - Layout constants and helpers for translating offsets into chunk/block indices.
//! - Slice descriptors plus utilities for turning them into block spans.
//! - A lightweight in-memory index used for demos/single-node development (persistent tracking should come from `meta`).
//!
//! Note: other modules in the repo are still placeholders; this module focuses purely on calculations and basic data structures.

#![allow(unused_imports)]

pub mod cache;
pub mod chunk;
pub mod reader;
pub mod slice;
pub mod span;
pub mod store;
pub mod util;
pub mod writer;

pub use chunk::{
    ChunkLayout, DEFAULT_BLOCK_SIZE, DEFAULT_CHUNK_SIZE, chunk_index_of, within_chunk_offset,
};
pub use slice::{BlockSpan, SliceDesc};
pub use span::{BlockTag, ChunkTag, PageTag, Span, SpanTag};
pub use store::{BlockStore, InMemoryBlockStore, ObjectBlockStore, RustfsBlockStore, S3BlockStore};
pub use util::ChunkSpan;
