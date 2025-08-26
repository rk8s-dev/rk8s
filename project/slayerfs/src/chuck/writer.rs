//! ChunkWriter：将写入缓冲按 block 拆分并写入块存储。

use super::slice::SliceDesc;
use super::store::BlockStore;
use super::{chunk::ChunkLayout, slice::BlockSpan};

pub struct ChunkWriter<'a, S: BlockStore> {
    layout: ChunkLayout,
    chunk_id: i64,
    store: &'a mut S,
}

impl<'a, S: BlockStore> ChunkWriter<'a, S> {
    pub fn new(layout: ChunkLayout, chunk_id: i64, store: &'a mut S) -> Self {
        Self {
            layout,
            chunk_id,
            store,
        }
    }

    /// 将一段位于 chunk 内的写入（offset+buf）拆分为若干 block 写入。
    pub async fn write(&mut self, offset_in_chunk: u64, buf: &[u8]) -> SliceDesc {
        let slice = SliceDesc {
            slice_id: 0,
            chunk_id: self.chunk_id,
            offset: offset_in_chunk,
            length: buf.len() as u32,
        };
        let spans: Vec<BlockSpan> = slice.block_spans(self.layout);
        let mut cursor = 0usize;
        for sp in spans {
            let take = sp.len_in_block as usize;
            let data = &buf[cursor..cursor + take];
            self.store
                .write_block_range(
                    self.chunk_id,
                    sp.block_index,
                    sp.offset_in_block,
                    data,
                    self.layout,
                )
                .await;
            cursor += take;
        }
        slice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::chunk::DEFAULT_BLOCK_SIZE;
    use crate::chuck::store::{BlockStore, InMemoryBlockStore};

    #[tokio::test]
    async fn test_writer_cross_blocks() {
        let layout = ChunkLayout::default();
        let mut store = InMemoryBlockStore::new();
        let mut writer = ChunkWriter::new(layout, 1, &mut store);

        // 写入从半个 block 开始，长度为一个半 block
        let half = (layout.block_size / 2) as usize;
        let len = layout.block_size as usize + half;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate().take(len) {
            *b = (i % 251) as u8; // 非平凡数据
        }
        let slice = writer.write(half as u64, &data).await;
        assert_eq!(slice.offset, half as u64);
        assert_eq!(slice.length as usize, len);

        // 读出并校验（复用 read_at）
        let mut out = Vec::with_capacity(len);
        // 第一块后半
        out.extend(
            store
                .read_block_range(
                    1,
                    0,
                    DEFAULT_BLOCK_SIZE / 2,
                    (DEFAULT_BLOCK_SIZE / 2) as usize,
                    layout,
                )
                .await,
        );
        // 第二整块
        out.extend(
            store
                .read_block_range(1, 1, 0, layout.block_size as usize, layout)
                .await,
        );

        assert_eq!(out, data);
    }
}
