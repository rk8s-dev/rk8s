//! ChunkReader：根据 offset/len 从块存储读取数据，支持跨块与洞零填充。

use super::chunk::ChunkLayout;
use super::slice::SliceDesc;
use super::store::BlockStore;

pub struct ChunkReader<'a, S: BlockStore> {
    layout: ChunkLayout,
    chunk_id: i64,
    store: &'a S,
}

impl<'a, S: BlockStore> ChunkReader<'a, S> {
    pub fn new(layout: ChunkLayout, chunk_id: i64, store: &'a S) -> Self {
        Self {
            layout,
            chunk_id,
            store,
        }
    }

    pub async fn read(&self, offset_in_chunk: u64, len: usize) -> Vec<u8> {
        if len == 0 {
            return Vec::new();
        }
        let slice = SliceDesc {
            slice_id: 0,
            chunk_id: self.chunk_id,
            offset: offset_in_chunk,
            length: len as u32,
        };
        let spans = slice.block_spans(self.layout);
        let mut out = Vec::with_capacity(len);
        for sp in spans {
            let part = self
                .store
                .read_block_range(
                    self.chunk_id,
                    sp.block_index,
                    sp.offset_in_block,
                    sp.len_in_block as usize,
                    self.layout,
                )
                .await;
            out.extend(part);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::store::InMemoryBlockStore;
    use crate::chuck::writer::ChunkWriter;

    #[tokio::test]
    async fn test_reader_zero_fills_holes() {
        let layout = ChunkLayout::default();
        let mut store = InMemoryBlockStore::new();
        // 只写第二个 block 的前半
        {
            let mut w = ChunkWriter::new(layout, 7, &mut store);
            let buf = vec![1u8; (layout.block_size / 2) as usize];
            w.write(layout.block_size as u64, &buf).await;
        }
        let r = ChunkReader::new(layout, 7, &store);
        // 读取从第一个 block 后半到第二个 block 前半，长度=block
        let off = (layout.block_size / 2) as u64;
        let res = r.read(off, layout.block_size as usize).await;
        assert_eq!(res.len(), layout.block_size as usize);
        // 前半应为 0 填充，后半应为 1
        assert!(
            res[..(layout.block_size / 2) as usize]
                .iter()
                .all(|&b| b == 0)
        );
        assert!(
            res[(layout.block_size / 2) as usize..]
                .iter()
                .all(|&b| b == 1)
        );
    }
}
