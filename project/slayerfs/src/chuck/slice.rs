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

/// Basic slice descriptor for a chunk-local contiguous range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SliceDesc {
    pub slice_id: u64,
    pub chunk_id: u64,
    /// Offset relative to the start of the chunk (bytes).
    pub offset: u32,
    /// Length in bytes.
    pub length: u32,
}

pub fn block_span_iter(desc: SliceDesc, layout: ChunkLayout) -> impl Iterator<Item = BlockSpan> {
    let chunk_span = Span::<ChunkTag>::new(0, desc.offset, desc.length);
    chunk_span.split_into::<BlockTag>(layout.chunk_size, layout.block_size as u64, true)
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
            length: DEFAULT_BLOCK_SIZE / 2,
        };
        let spans: Vec<BlockSpan> = block_span_iter(s, layout).collect();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].index, 0);
        assert_eq!(spans[0].offset, 0);
        assert_eq!(spans[0].len, DEFAULT_BLOCK_SIZE / 2);
    }

    #[test]
    fn test_cross_two_blocks() {
        let layout = ChunkLayout::default();
        let half = layout.block_size / 2;
        let s = SliceDesc {
            slice_id: 1,
            chunk_id: 1,
            offset: half,
            length: layout.block_size,
        };
        let spans: Vec<BlockSpan> = block_span_iter(s, layout).collect();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].index, 0);
        assert_eq!(spans[0].offset, (layout.block_size / 2));
        assert_eq!(spans[0].len, layout.block_size / 2);
        assert_eq!(spans[1].index, 1);
        assert_eq!(spans[1].offset, 0);
        assert_eq!(spans[1].len, layout.block_size / 2);
    }
}
