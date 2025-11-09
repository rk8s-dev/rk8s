//! Chunk layout and indexing utilities.
//!
//! - JuiceFS-style fixed-size chunk/block partitioning.
//! - Helpers to compute (chunk_index, offset_in_chunk) from file offsets.
//! - `ChunkLayout` for custom sizes with sensible defaults.
//! - `ChunkKey` placeholders for in-memory indexing (to be replaced by `meta`).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Default chunk size (64 MiB).
pub const DEFAULT_CHUNK_SIZE: u64 = 64 * 1024 * 1024;
/// Default block size (4 MiB).
pub const DEFAULT_BLOCK_SIZE: u32 = 4 * 1024 * 1024;

/// With the default layout, return the zero-based chunk index for a file offset.
#[inline]
#[allow(dead_code)]
pub fn chunk_index_of(file_offset: u64) -> u64 {
    file_offset / DEFAULT_CHUNK_SIZE
}

/// With the default layout, return the intra-chunk offset for a file offset.
#[inline]
#[allow(dead_code)]
pub fn within_chunk_offset(file_offset: u64) -> u64 {
    file_offset % DEFAULT_CHUNK_SIZE
}

/// Layout parameters for chunks and blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkLayout {
    pub chunk_size: u64,
    pub block_size: u32,
}

impl Default for ChunkLayout {
    fn default() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
            block_size: DEFAULT_BLOCK_SIZE,
        }
    }
}

impl ChunkLayout {
    #[inline]
    #[allow(dead_code)]
    pub fn blocks_per_chunk(&self) -> u32 {
        let bs = self.block_size as u64;
        self.chunk_size.div_ceil(bs) as u32
    }

    #[inline]
    #[allow(dead_code)]
    pub fn chunk_index_of(&self, file_offset: u64) -> u64 {
        file_offset / self.chunk_size
    }

    #[inline]
    #[allow(dead_code)]
    pub fn within_chunk_offset(&self, file_offset: u64) -> u64 {
        file_offset % self.chunk_size
    }

    #[inline]
    #[allow(dead_code)]
    pub fn block_index_of(&self, offset_in_chunk: u64) -> u32 {
        (offset_in_chunk / self.block_size as u64) as u32
    }

    #[inline]
    pub fn within_block_offset(&self, offset_in_chunk: u64) -> u32 {
        (offset_in_chunk % self.block_size as u64) as u32
    }

    /// Return the file byte range [start, end) covered by a chunk index (end exclusive).
    #[inline]
    #[allow(dead_code)]
    pub fn chunk_byte_range(&self, chunk_index: u64) -> (u64, u64) {
        let start = chunk_index * self.chunk_size;
        let end = start + self.chunk_size;
        (start, end)
    }
}

/// Logical chunk key used by the in-memory index.
#[derive(Debug, Clone, Copy, Eq)]
#[allow(dead_code)]
pub struct ChunkKey {
    pub ino: i64,
    pub index: i32,
}

impl PartialEq for ChunkKey {
    fn eq(&self, other: &Self) -> bool {
        self.ino == other.ino && self.index == other.index
    }
}

impl Hash for ChunkKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ino.hash(state);
        self.index.hash(state);
    }
}

/// Placeholder for chunk metadata (e.g., checksum, timestamps).
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ChunkMeta {
    pub chunk_id: i64,
    pub ino: i64,
    pub index: i32,
}

impl ChunkMeta {
    #[allow(dead_code)]
    pub fn new(chunk_id: i64, ino: i64, index: i32) -> Self {
        Self {
            chunk_id,
            ino,
            index,
        }
    }
}

/// Simple in-memory index: track how many slices are committed for each chunk (demo only).
#[derive(Default)]
#[allow(dead_code)]
pub struct InMemoryChunkIndex {
    map: HashMap<ChunkKey, usize>,
}

#[allow(dead_code)]
impl InMemoryChunkIndex {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn incr_slice_count(&mut self, key: ChunkKey) {
        *self.map.entry(key).or_insert(0) += 1;
    }

    #[allow(dead_code)]
    pub fn get_slice_count(&self, key: &ChunkKey) -> usize {
        self.map.get(key).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_helpers() {
        let off = DEFAULT_CHUNK_SIZE + 123;
        assert_eq!(chunk_index_of(off), 1);
        assert_eq!(within_chunk_offset(off), 123);
    }

    #[test]
    fn test_layout_block_mapping() {
        let layout = ChunkLayout::default();
        let bs = layout.block_size as u64;

        let off = bs + (bs / 2);
        assert_eq!(layout.block_index_of(off), 1);
        assert_eq!(layout.within_block_offset(off), (bs / 2) as u32);
    }

    #[test]
    fn test_chunk_byte_range() {
        let layout = ChunkLayout::default();
        let (s0, e0) = layout.chunk_byte_range(0);
        assert_eq!(s0, 0);
        assert_eq!(e0, DEFAULT_CHUNK_SIZE);

        let (s1, e1) = layout.chunk_byte_range(1);
        assert_eq!(s1, DEFAULT_CHUNK_SIZE);
        assert_eq!(e1, DEFAULT_CHUNK_SIZE * 2);
    }

    #[test]
    fn test_in_memory_index() {
        let mut idx = InMemoryChunkIndex::new();
        let key = ChunkKey { ino: 1, index: 0 };
        assert_eq!(idx.get_slice_count(&key), 0);
        idx.incr_slice_count(key);
        idx.incr_slice_count(key);
        assert_eq!(idx.get_slice_count(&key), 2);
    }
}
