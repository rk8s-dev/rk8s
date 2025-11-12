//! Utility helpers for splitting file ranges into chunk-local spans.

use super::chunk::ChunkLayout;

/// A range within a chunk for a portion of the file span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkSpan {
    pub index: u64,
    pub offset: u64,
    pub len: usize,
}

/// Split `[file_offset, file_offset+len)` into chunk-local spans.
pub fn split_file_range_into_chunks(
    layout: ChunkLayout,
    mut file_offset: u64,
    len: usize,
) -> Vec<ChunkSpan> {
    let mut remaining = len as u64;
    let mut out = Vec::new();
    if remaining == 0 {
        return out;
    }

    while remaining > 0 {
        let ci = layout.chunk_index_of(file_offset);
        let off_in_chunk = layout.within_chunk_offset(file_offset);
        let cap = layout.chunk_size - off_in_chunk;
        let take = cap.min(remaining) as usize;
        out.push(ChunkSpan {
            index: ci,
            offset: off_in_chunk,
            len: take,
        });
        file_offset += take as u64;
        remaining -= take as u64;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_within_single_chunk() {
        let layout = ChunkLayout::default();
        let spans = split_file_range_into_chunks(layout, 123, 4096);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].index, 0);
        assert_eq!(spans[0].offset, 123);
        assert_eq!(spans[0].len, 4096);
    }

    #[test]
    fn test_split_across_two_chunks() {
        let layout = ChunkLayout::default();
        let start = layout.chunk_size - 10;
        let spans = split_file_range_into_chunks(layout, start, 100);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].index, 0);
        assert_eq!(spans[0].offset, layout.chunk_size - 10);
        assert_eq!(spans[0].len, 10);
        assert_eq!(spans[1].index, 1);
        assert_eq!(spans[1].offset, 0);
        assert_eq!(spans[1].len, 90);
    }

    #[test]
    fn test_zero_len() {
        let layout = ChunkLayout::default();
        let spans = split_file_range_into_chunks(layout, 0, 0);
        assert!(spans.is_empty());
    }
}
