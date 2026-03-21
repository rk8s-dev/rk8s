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
    layout::ChunkLayout,
    span::{BlockTag, ChunkTag, Span},
};
use crate::chunk::BlockStore;
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

impl SliceDesc {
    /// Calculate the real fragmentation ratio using interval merging.
    ///
    /// Fragmentation = (total_slice_size - deduplicated_coverage) / total_slice_size
    ///
    /// This correctly accounts for partially overlapping slices, unlike
    /// counting only fully-covered slices which would undercount fragmentation.
    ///
    /// Example:
    /// - Slice A: offset=0, length=100
    /// - Slice B: offset=50, length=100
    /// - total_size = 200, merged_coverage = 150 (union of [0,100) and [50,150))
    /// - fragmentation = (200 - 150) / 200 = 0.25
    pub fn calculate_fragmentation(slices: &[SliceDesc]) -> f64 {
        if slices.is_empty() {
            return 0.0;
        }
        let total_size: u64 = slices.iter().map(|s| s.length).sum();
        if total_size == 0 {
            return 0.0;
        }

        // Sort intervals by start offset for sweep-line merge
        let mut intervals: Vec<(u64, u64)> = slices
            .iter()
            .filter(|s| s.length > 0)
            .map(|s| (s.offset, s.offset + s.length))
            .collect();
        if intervals.is_empty() {
            return 0.0;
        }
        intervals.sort();

        // Merge overlapping intervals to compute deduplicated coverage
        let mut merged_coverage: u64 = 0;
        let (mut cur_start, mut cur_end) = intervals[0];
        for &(s, e) in &intervals[1..] {
            if s <= cur_end {
                cur_end = cur_end.max(e);
            } else {
                merged_coverage += cur_end - cur_start;
                cur_start = s;
                cur_end = e;
            }
        }
        merged_coverage += cur_end - cur_start;

        (total_size - merged_coverage) as f64 / total_size as f64
    }

    /// Remove slices that are fully covered by newer slices.
    ///
    /// This is a conservative strategy for safe deletion: only slices whose
    /// entire range `[offset, offset+length)` is contained within a single
    /// newer slice are removed. Partially covered slices are kept intact
    /// because their block data addressing depends on the original offset.
    ///
    /// Ordering: slices with higher `slice_id` are considered newer.
    pub fn remove_fully_covered(slices: &[SliceDesc]) -> Vec<SliceDesc> {
        if slices.is_empty() {
            return vec![];
        }

        // Sort by slice_id descending (newest first)
        let mut sorted: Vec<SliceDesc> = slices.to_vec();
        sorted.sort_by_key(|s| std::cmp::Reverse(s.slice_id));

        let mut covered_ranges: Vec<(u64, u64)> = Vec::new();
        let mut result: Vec<SliceDesc> = Vec::new();

        for slice in sorted {
            let start = slice.offset;
            let end = slice.offset + slice.length;

            // Check if fully covered by any single newer slice's range
            let is_fully_covered = covered_ranges
                .iter()
                .any(|&(cs, ce)| start >= cs && end <= ce);

            if !is_fully_covered {
                result.push(slice);
            }

            covered_ranges.push((start, end));
        }

        result.sort_by_key(|s| s.offset);
        result
    }

    /// Find slice IDs present in original but not in merged
    pub fn find_replaced_ids(original: &[SliceDesc], merged: &[SliceDesc]) -> Vec<u64> {
        let merged_ids: std::collections::HashSet<u64> =
            merged.iter().map(|s| s.slice_id).collect();
        original
            .iter()
            .filter(|s| !merged_ids.contains(&s.slice_id))
            .map(|s| s.slice_id)
            .collect()
    }

    /// Encode replaced slices into the delayed deletion binary format.
    pub fn encode_delayed_data(slices: &[SliceDesc], replaced_ids: &[u64]) -> Vec<u8> {
        let replaced_set: std::collections::HashSet<u64> = replaced_ids.iter().copied().collect();
        let mut buf = Vec::with_capacity(replaced_ids.len() * 20);
        for s in slices {
            if replaced_set.contains(&s.slice_id) {
                buf.extend_from_slice(&s.slice_id.to_le_bytes());
                buf.extend_from_slice(&s.offset.to_le_bytes());
                let size = s.length.min(u32::MAX as u64) as u32;
                buf.extend_from_slice(&size.to_le_bytes());
            }
        }
        buf
    }

    /// Decode the delayed deletion binary format into (slice_id, offset, size) tuples.
    pub fn decode_delayed_data(data: &[u8]) -> Option<Vec<(u64, u64, u32)>> {
        if !data.len().is_multiple_of(20) {
            return None;
        }
        let mut out = Vec::with_capacity(data.len() / 20);
        for chunk in data.chunks_exact(20) {
            let slice_id = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
            let offset = u64::from_le_bytes(chunk[8..16].try_into().unwrap());
            let size = u32::from_le_bytes(chunk[16..20].try_into().unwrap());
            out.push((slice_id, offset, size));
        }
        Some(out)
    }
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
    use crate::chunk::layout::DEFAULT_BLOCK_SIZE;

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

    // ==================== calculate_fragmentation tests ====================

    fn make_slice(id: u64, offset: u64, length: u64) -> SliceDesc {
        SliceDesc {
            slice_id: id,
            chunk_id: 1,
            offset,
            length,
        }
    }

    #[test]
    fn test_fragmentation_empty() {
        assert_eq!(SliceDesc::calculate_fragmentation(&[]), 0.0);
    }

    #[test]
    fn test_fragmentation_single_slice() {
        let slices = vec![make_slice(1, 0, 100)];
        assert_eq!(SliceDesc::calculate_fragmentation(&slices), 0.0);
    }

    #[test]
    fn test_fragmentation_no_overlap() {
        // [0,100) + [200,300) → total=200, coverage=200, frag=0
        let slices = vec![make_slice(1, 0, 100), make_slice(2, 200, 100)];
        assert_eq!(SliceDesc::calculate_fragmentation(&slices), 0.0);
    }

    #[test]
    fn test_fragmentation_partial_overlap() {
        // [0,100) + [50,150) → total=200, coverage=150, frag=50/200=0.25
        let slices = vec![make_slice(1, 0, 100), make_slice(2, 50, 100)];
        let frag = SliceDesc::calculate_fragmentation(&slices);
        assert!((frag - 0.25).abs() < 1e-9, "expected 0.25, got {frag}");
    }

    #[test]
    fn test_fragmentation_full_overlap() {
        // [0,100) + [0,100) → total=200, coverage=100, frag=100/200=0.5
        let slices = vec![make_slice(1, 0, 100), make_slice(2, 0, 100)];
        let frag = SliceDesc::calculate_fragmentation(&slices);
        assert!((frag - 0.5).abs() < 1e-9, "expected 0.5, got {frag}");
    }

    #[test]
    fn test_fragmentation_superset_coverage() {
        // [10,60) + [0,100) → total=150, coverage=100, frag=50/150≈0.333
        let slices = vec![make_slice(1, 10, 50), make_slice(2, 0, 100)];
        let frag = SliceDesc::calculate_fragmentation(&slices);
        let expected = 50.0 / 150.0;
        assert!(
            (frag - expected).abs() < 1e-9,
            "expected {expected}, got {frag}"
        );
    }

    #[test]
    fn test_fragmentation_chain_overlap() {
        // [0,100) + [50,150) + [100,200) → total=300, coverage=200, frag=100/300≈0.333
        let slices = vec![
            make_slice(1, 0, 100),
            make_slice(2, 50, 100),
            make_slice(3, 100, 100),
        ];
        let frag = SliceDesc::calculate_fragmentation(&slices);
        let expected = 100.0 / 300.0;
        assert!(
            (frag - expected).abs() < 1e-9,
            "expected {expected}, got {frag}"
        );
    }

    #[test]
    fn test_fragmentation_zero_length_ignored() {
        let slices = vec![make_slice(1, 0, 0), make_slice(2, 0, 0)];
        assert_eq!(SliceDesc::calculate_fragmentation(&slices), 0.0);
    }

    // ==================== remove_fully_covered tests ====================

    #[test]
    fn test_remove_fully_covered_empty() {
        assert!(SliceDesc::remove_fully_covered(&[]).is_empty());
    }

    #[test]
    fn test_remove_fully_covered_no_overlap() {
        let slices = vec![make_slice(1, 0, 50), make_slice(2, 100, 50)];
        let result = SliceDesc::remove_fully_covered(&slices);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_remove_fully_covered_partial_overlap_kept() {
        // [0,100) + [50,150) → neither fully covers the other
        let slices = vec![make_slice(1, 0, 100), make_slice(2, 50, 100)];
        let result = SliceDesc::remove_fully_covered(&slices);
        assert_eq!(result.len(), 2, "partial overlap: both kept");
    }

    #[test]
    fn test_remove_fully_covered_full_coverage() {
        // [10,60) fully inside [0,100) — slice 1 removed (newer slice 2 covers it)
        let slices = vec![make_slice(1, 10, 50), make_slice(2, 0, 100)];
        let result = SliceDesc::remove_fully_covered(&slices);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slice_id, 2);
    }

    #[test]
    fn test_remove_fully_covered_exact_same_range() {
        // [0,100) + [0,100) — older one removed
        let slices = vec![make_slice(1, 0, 100), make_slice(2, 0, 100)];
        let result = SliceDesc::remove_fully_covered(&slices);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slice_id, 2);
    }

    #[test]
    fn test_remove_fully_covered_chain() {
        // A⊂B⊂C → only C survives
        let slices = vec![
            make_slice(1, 10, 20),
            make_slice(2, 5, 50),
            make_slice(3, 0, 100),
        ];
        let result = SliceDesc::remove_fully_covered(&slices);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slice_id, 3);
    }

    #[test]
    fn test_remove_fully_covered_sorted_by_offset() {
        let slices = vec![
            make_slice(3, 200, 50),
            make_slice(1, 0, 50),
            make_slice(2, 100, 50),
        ];
        let result = SliceDesc::remove_fully_covered(&slices);
        for i in 1..result.len() {
            assert!(result[i].offset >= result[i - 1].offset, "not sorted");
        }
    }
}
