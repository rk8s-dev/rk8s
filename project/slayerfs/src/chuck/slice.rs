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

use super::chunk::ChunkLayout;
use crate::chuck::BlockStore;
use crate::meta::MetaStore;
use anyhow::Context;
use std::marker::PhantomData;

/// Portion of a slice that resides inside a single block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSpan {
    pub index: u32,
    /// Start offset within the block (bytes).
    pub offset: u32,
    /// Length covered inside the block (bytes).
    pub len: u32,
}

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

impl SliceDesc {
    /// Map this slice into a list of block spans.
    ///
    /// - Returns an empty list when `length == 0`.
    /// - Each returned span stays within block bounds (`offset + len <= block_size`).
    /// - The spans cover the entire `[offset, offset+length)` range.
    pub fn block_spans(&self, layout: ChunkLayout) -> Vec<BlockSpan> {
        if self.length == 0 {
            return Vec::new();
        }

        let mut spans = Vec::new();
        let mut remaining = self.length as u64;
        let mut cur_off_in_chunk = self.offset as u64;

        while remaining > 0 {
            // Determine the block index and within-block offset for the current position
            let bi = layout.block_index_of(cur_off_in_chunk);
            let wbo = layout.within_block_offset(cur_off_in_chunk) as u64;
            // Remaining capacity in this block
            let cap = layout.block_size as u64 - wbo;
            // Take the min of remaining capacity and remaining length
            let take = cap.min(remaining);
            spans.push(BlockSpan {
                index: bi,
                offset: wbo as u32,
                len: take as u32,
            });
            // Advance to the next starting point
            cur_off_in_chunk += take;
            remaining -= take;
        }
        spans
    }
}

pub struct Write;

pub struct Read;

pub struct SliceIO<'a, State, B> {
    desc: SliceDesc,
    layout: ChunkLayout,
    store: &'a B,
    _io_mode: PhantomData<State>,
}

impl<'a, State, B> SliceIO<'a, State, B>
where
    B: BlockStore,
{
    pub fn new(desc: SliceDesc, layout: ChunkLayout, store: &'a B) -> SliceIO<'a, State, B> {
        Self {
            desc,
            layout,
            store,
            _io_mode: PhantomData,
        }
    }
}

impl<'a, B> SliceIO<'a, Write, B>
where
    B: BlockStore,
{
    pub async fn write(&self, buf: &[u8]) -> anyhow::Result<SliceDesc> {
        let spans = self.desc.block_spans(self.layout);
        let mut cursor = 0;

        for (index, span) in spans.iter().enumerate() {
            let take = span.len as usize;
            let data = &buf[cursor..(cursor + take)];

            self.store
                .write_range((self.desc.slice_id, index as u32), span.offset, data)
                .await?;
            cursor += take;
        }
        Ok(self.desc)
    }
}

impl<'a, B> SliceIO<'a, Read, B>
where
    B: BlockStore,
{
    pub async fn read(&self, buf: &mut [u8]) -> anyhow::Result<()> {
        debug_assert_eq!(buf.len(), self.desc.length as usize);
        let spans = self.desc.block_spans(self.layout);
        let mut cursor = 0usize;

        for (index, span) in spans.iter().enumerate() {
            let take = span.len as usize;
            let out = &mut buf[cursor..cursor + take];
            self.store
                .read_range((self.desc.slice_id, index as u32), span.offset, out)
                .await?;
            cursor += take;
        }
        Ok(())
    }
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
        let spans = s.block_spans(layout);
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
        let spans = s.block_spans(layout);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].index, 0);
        assert_eq!(spans[0].offset, (layout.block_size / 2));
        assert_eq!(spans[0].len, layout.block_size / 2);
        assert_eq!(spans[1].index, 1);
        assert_eq!(spans[1].offset, 0);
        assert_eq!(spans[1].len, layout.block_size / 2);
    }
}
