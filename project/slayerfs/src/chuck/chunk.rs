//! Chunk 布局与索引工具
//!
//! - 提供与 JuiceFS 类似的 chunk/block 固定大小切分方案。
//! - 提供根据文件偏移计算 (chunk_index, offset_in_chunk) 的函数。
//! - 提供 `ChunkLayout` 以自定义大小；默认常量用于快速上手。
//! - 提供 `ChunkKey` 与内存级索引占位（后续由 `meta` 层替换）。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// 默认 Chunk 尺寸（64 MiB）。
pub const DEFAULT_CHUNK_SIZE: u64 = 64 * 1024 * 1024;
/// 默认 Block 尺寸（4 MiB）。
pub const DEFAULT_BLOCK_SIZE: u32 = 4 * 1024 * 1024;

/// 使用默认布局：返回文件偏移所在的 chunk 序号（从 0 开始）。
#[inline]
pub fn chunk_index_of(file_offset: u64) -> u64 {
    file_offset / DEFAULT_CHUNK_SIZE
}

/// 使用默认布局：返回文件偏移在其 chunk 内的偏移量。
#[inline]
pub fn within_chunk_offset(file_offset: u64) -> u64 {
    file_offset % DEFAULT_CHUNK_SIZE
}

/// Chunk 与 Block 的布局参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkLayout {
    pub chunk_size: u64,
    pub block_size: u32,
}

impl Default for ChunkLayout {
    fn default() -> Self {
        Self { chunk_size: DEFAULT_CHUNK_SIZE, block_size: DEFAULT_BLOCK_SIZE }
    }
}

impl ChunkLayout {
    #[inline]
    #[allow(dead_code)]
    pub fn blocks_per_chunk(&self) -> u32 {
        let bs = self.block_size as u64;
        ((self.chunk_size + bs - 1) / bs) as u32
    }

    #[inline]
    pub fn chunk_index_of(&self, file_offset: u64) -> u64 {
        file_offset / self.chunk_size
    }

    #[inline]
    pub fn within_chunk_offset(&self, file_offset: u64) -> u64 {
        file_offset % self.chunk_size
    }

    #[inline]
    pub fn block_index_of(&self, offset_in_chunk: u64) -> u32 {
        (offset_in_chunk / self.block_size as u64) as u32
    }

    #[inline]
    pub fn within_block_offset(&self, offset_in_chunk: u64) -> u32 {
        (offset_in_chunk % self.block_size as u64) as u32
    }

    /// 返回给定 chunk 索引对应的文件级字节范围 [start, end)（end 为开区间）。
    #[inline]
    pub fn chunk_byte_range(&self, chunk_index: u64) -> (u64, u64) {
        let start = chunk_index * self.chunk_size;
        let end = start + self.chunk_size;
        (start, end)
    }
}

/// 逻辑 chunk 的键（内存索引使用）。
#[derive(Debug, Clone, Copy, Eq)]
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

/// chunk 元数据占位（未来可扩展如校验、时间戳等）。
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
        Self { chunk_id, ino, index }
    }
}

/// 简单的内存索引：维护每个 chunk 下已提交的 slice 数量（示例用）。
#[derive(Default)]
pub struct InMemoryChunkIndex {
    map: HashMap<ChunkKey, usize>,
}

impl InMemoryChunkIndex {
    pub fn new() -> Self { Self { map: HashMap::new() } }

    pub fn incr_slice_count(&mut self, key: ChunkKey) {
        *self.map.entry(key).or_insert(0) += 1;
    }

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
