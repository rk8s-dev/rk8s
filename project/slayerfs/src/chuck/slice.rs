//! Slice lifecycle and block mapping utilities.
//!
//! Goal: take a contiguous region (slice) inside a chunk and split it into block-aligned
//! fragments (`BlockSpan`) so the block store can write/read block by block.
//!
//! Terminology recap:
//! - Chunk: logical contiguous region (e.g., 64 MiB) further divided into equal-sized blocks (e.g., 4 MiB).
//! - Block: fixed-size portion inside a chunk, the smallest IO unit for object storage.
//! - Slice: arbitrary contiguous range within a chunk that may start/end mid-block.
//!
//! Mapping properties:
//! - The generated [`BlockSpan`] list is monotonic by block index (`BlockSpan::index`).
//! - Spans within a block never overlap and adjacent blocks are contiguous.
//! - The sum of all `len_in_block` equals the slice `length`.
//! - Complexity O(number of covered blocks) in time and space.
//!
//! Visual guide (S marks the covered region):
//!
//!   Block 0: |------SSSS|  (start at within-block offset)
//!   Block 1: |SSSSSSSSS|
//!   Block 2: |SSSS------|  (stop before block_size)
//!
//! Note: this module assumes the provided slice stays inside a single chunk; cross-chunk validation is not performed.

use super::{
    chunk::ChunkLayout,
    span::{BlockTag, ChunkTag, Span},
};
use crate::chuck::BlockStore;
use anyhow::Context;
use std::marker::PhantomData;

/// Portion of a slice that resides inside a single block.
pub type BlockSpan = Span<BlockTag>;

/// Byte offset relative to the start of a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ChunkOffset(pub u64);

impl ChunkOffset {
    pub const fn new(offset: u64) -> Self {
        Self(offset)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl From<u64> for ChunkOffset {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<ChunkOffset> for u64 {
    fn from(value: ChunkOffset) -> Self {
        value.0
    }
}

/// Byte offset relative to the start of a slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SliceOffset(pub u64);

impl SliceOffset {
    pub const fn new(offset: u64) -> Self {
        Self(offset)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl From<u64> for SliceOffset {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<SliceOffset> for u64 {
    fn from(value: SliceOffset) -> Self {
        value.0
    }
}

/// Basic slice descriptor for a chunk-local contiguous range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(
    feature = "rkyv-serialization",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[cfg_attr(feature = "rkyv-serialization", rkyv(compare(PartialEq)))]
pub struct SliceDesc {
    pub slice_id: u64,
    pub chunk_id: u64,
    /// Offset relative to the start of the chunk (bytes).
    pub offset: u64,
    /// Length in bytes.
    pub length: u64,
}

pub fn block_span_iter_range(
    offset: u64,
    length: u64,
    layout: ChunkLayout,
) -> impl Iterator<Item = BlockSpan> {
    let chunk_span = Span::<ChunkTag>::new(0, offset, length);
    chunk_span.split_into::<BlockTag>(layout.chunk_size, layout.block_size as u64, true)
}

#[allow(dead_code)]
pub fn block_span_iter_chunk(
    offset: ChunkOffset,
    length: u64,
    layout: ChunkLayout,
) -> impl Iterator<Item = BlockSpan> {
    block_span_iter_range(offset.get(), length, layout)
}

pub fn block_span_iter_slice(
    offset: SliceOffset,
    length: u64,
    layout: ChunkLayout,
) -> impl Iterator<Item = BlockSpan> {
    block_span_iter_range(offset.get(), length, layout)
}

pub fn key_for_slice(chunk_id: u64) -> String {
    format!("slices/{chunk_id}")
}

#[allow(dead_code)]
pub fn key_for_block_of_slice(slice_id: u64, index: u64) -> String {
    format!("{slice_id}/{index}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::chunk::DEFAULT_BLOCK_SIZE;

    #[test]
    fn test_single_block_span() {
        let layout = ChunkLayout::default();
        let s = SliceDesc {
            slice_id: 1,
            chunk_id: 1,
            offset: 0,
            length: (DEFAULT_BLOCK_SIZE / 2) as u64,
        };
        let spans: Vec<BlockSpan> =
            block_span_iter_chunk(s.offset.into(), s.length, layout).collect();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].index, 0);
        assert_eq!(spans[0].offset, 0);
        assert_eq!(spans[0].len, (DEFAULT_BLOCK_SIZE / 2) as u64);
    }

    #[test]
    fn test_cross_two_blocks() {
        let layout = ChunkLayout::default();
        let half = layout.block_size / 2;
        let s = SliceDesc {
            slice_id: 1,
            chunk_id: 1,
            offset: half as u64,
            length: layout.block_size as u64,
        };
        let spans: Vec<BlockSpan> =
            block_span_iter_chunk(s.offset.into(), s.length, layout).collect();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].index, 0);
        assert_eq!(spans[0].offset, (layout.block_size / 2) as u64);
        assert_eq!(spans[0].len, (layout.block_size / 2) as u64);
        assert_eq!(spans[1].index, 1);
        assert_eq!(spans[1].offset, 0);
        assert_eq!(spans[1].len, (layout.block_size / 2) as u64);
    }

    #[test]
    fn test_slice_desc_serialization_roundtrip() {
        let desc = SliceDesc {
            slice_id: 1,
            chunk_id: 2,
            offset: 100,
            length: 4096,
        };
        let bytes = crate::meta::serialization::serialize_meta(&desc).unwrap();
        let recovered: SliceDesc = crate::meta::serialization::deserialize_meta(&bytes).unwrap();
        assert_eq!(desc, recovered);
    }

    #[test]
    fn test_slice_desc_json_backward_compat() {
        // Old JSON format must still work - MUST use deserialize_meta (not serde_json::from_str)
        let json_bytes = br#"{"slice_id":1,"chunk_id":2,"offset":100,"length":4096}"#;
        let desc: SliceDesc = crate::meta::serialization::deserialize_meta(json_bytes).unwrap();
        assert_eq!(desc.slice_id, 1);
        assert_eq!(desc.chunk_id, 2);
        assert_eq!(desc.offset, 100);
        assert_eq!(desc.length, 4096);
    }
}
