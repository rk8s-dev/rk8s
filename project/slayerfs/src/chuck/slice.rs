//! Slice 生命周期与 block 映射
//!
//! 目标：把位于某个 Chunk 内的一段连续字节区间（Slice），拆分成按 Block 对齐的
//! 若干片段（[`BlockSpan`]），供块存储层按块写入/读取。
//!
//! 术语回顾：
//! - Chunk：逻辑连续的数据区域（例如 64MiB），被划分为多个等长 Block（例如 4MiB）。
//! - Block：Chunk 内的固定大小分片，是对象存储的最小写入/读取单元。
//! - Slice：位于某个 Chunk 内、任意偏移与长度的连续范围，不一定与块边界对齐。
//!
//! 映射结果特性：
//! - 生成的 [`BlockSpan`] 列表按 `block_index` 单调递增；
//! - 各片段在同一块内不重叠，且跨块相邻；
//! - 所有 `len_in_block` 之和等于 Slice 的 `length`；
//! - 时间复杂度 O(跨越的块数)，额外空间 O(跨越的块数)。
//!
//! 小示意图（S 表示 Slice 覆盖部分）：
//!
//!   Block 0: |------SSSS|  (从块内偏移 wbo 开始)
//!   Block 1: |SSSSSSSSS|
//!   Block 2: |SSSS------|  (到块内偏移 < block_size 结束)
//!
//! 注意：本模块假定传入的 Slice 完全位于一个 Chunk 之内，不做跨 Chunk 校验。

use super::chunk::ChunkLayout;

/// 一个切片在某个 block 上覆盖的范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSpan {
    pub block_index: u32,
    /// 在该 block 内的起始偏移（字节）。
    pub offset_in_block: u32,
    /// 在该 block 内覆盖的长度（字节）。
    pub len_in_block: u32,
}

/// Slice 的基本描述（位于某个 chunk 内的连续范围）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SliceDesc {
    pub slice_id: i64,
    pub chunk_id: i64,
    /// 相对 chunk 起始的偏移（字节）。
    pub offset: u64,
    /// 长度（字节）。
    pub length: u32,
}

impl SliceDesc {
    /// 将当前 slice 映射为一组 block 跨度。
    ///
    /// - 当 `length == 0` 时返回空列表；
    /// - 每个返回项的 `(block_index, offset_in_block, len_in_block)` 均满足
    ///   `offset_in_block + len_in_block <= block_size`；
    /// - 返回序列覆盖从 `offset` 起始、长度为 `length` 的全部范围。
    pub fn block_spans(&self, layout: ChunkLayout) -> Vec<BlockSpan> {
        if self.length == 0 {
            return Vec::new();
        }

        let mut spans = Vec::new();
        let mut remaining = self.length as u64;
        let mut cur_off_in_chunk = self.offset;

        while remaining > 0 {
            // 计算当前偏移所处的块索引与块内偏移
            let bi = layout.block_index_of(cur_off_in_chunk);
            let wbo = layout.within_block_offset(cur_off_in_chunk) as u64;
            // 本块剩余可容纳的字节数
            let cap = layout.block_size as u64 - wbo;
            // 实际取本块容量与剩余长度的较小者
            let take = cap.min(remaining);
            spans.push(BlockSpan {
                block_index: bi,
                offset_in_block: wbo as u32,
                len_in_block: take as u32,
            });
            // 推进到下一个起点
            cur_off_in_chunk += take;
            remaining -= take;
        }
        spans
    }
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
        assert_eq!(spans[0].block_index, 0);
        assert_eq!(spans[0].offset_in_block, 0);
        assert_eq!(spans[0].len_in_block, DEFAULT_BLOCK_SIZE / 2);
    }

    #[test]
    fn test_cross_two_blocks() {
        let layout = ChunkLayout::default();
        let half = (layout.block_size / 2) as u64;
        let s = SliceDesc {
            slice_id: 1,
            chunk_id: 1,
            offset: half,
            length: layout.block_size,
        };
        let spans = s.block_spans(layout);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].block_index, 0);
        assert_eq!(spans[0].offset_in_block, (layout.block_size / 2));
        assert_eq!(spans[0].len_in_block, layout.block_size / 2);
        assert_eq!(spans[1].block_index, 1);
        assert_eq!(spans[1].offset_in_block, 0);
        assert_eq!(spans[1].len_in_block, layout.block_size / 2);
    }
}
