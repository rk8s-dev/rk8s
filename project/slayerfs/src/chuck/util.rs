//! 通用工具：将文件范围按 chunk 进行拆分。

use super::chunk::ChunkLayout;

/// 文件范围在某个 chunk 内的一段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkSpan {
    pub chunk_index: u64,
    pub offset_in_chunk: u64,
    pub len: usize,
}

/// 将文件的 [file_offset, file_offset+len) 拆分为若干 chunk 局部范围。
pub fn split_file_range_into_chunks(
    layout: ChunkLayout,
    mut file_offset: u64,
    len: usize,
) -> Vec<ChunkSpan> {
    let mut remaining = len as u64;
    let mut out = Vec::new();
    if remaining == 0 { return out; }

    while remaining > 0 {
        let ci = layout.chunk_index_of(file_offset);
        let off_in_chunk = layout.within_chunk_offset(file_offset);
        let cap = layout.chunk_size - off_in_chunk;
        let take = cap.min(remaining) as usize;
        out.push(ChunkSpan { chunk_index: ci, offset_in_chunk: off_in_chunk, len: take });
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
        assert_eq!(spans[0].chunk_index, 0);
        assert_eq!(spans[0].offset_in_chunk, 123);
        assert_eq!(spans[0].len, 4096);
    }

    #[test]
    fn test_split_across_two_chunks() {
        let layout = ChunkLayout::default();
        let start = layout.chunk_size - 10;
        let spans = split_file_range_into_chunks(layout, start, 100);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].chunk_index, 0);
        assert_eq!(spans[0].offset_in_chunk, layout.chunk_size - 10);
        assert_eq!(spans[0].len, 10);
        assert_eq!(spans[1].chunk_index, 1);
        assert_eq!(spans[1].offset_in_chunk, 0);
        assert_eq!(spans[1].len, 90);
    }

    #[test]
    fn test_zero_len() {
        let layout = ChunkLayout::default();
        let spans = split_file_range_into_chunks(layout, 0, 0);
        assert!(spans.is_empty());
    }
}
