//! ChunkReader: fetch data from blocks according to offset/length, handling gaps with zeros.

use super::chunk::ChunkLayout;
use super::slice::SliceDesc;
use super::store::BlockStore;
use anyhow::Result;

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

    pub async fn read(&self, offset_in_chunk: u64, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
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
                .read_range(
                    (self.chunk_id, sp.block_index),
                    sp.offset_in_block,
                    sp.len_in_block as usize,
                )
                .await?;
            out.extend(part);
        }
        Ok(out)
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
        let store = InMemoryBlockStore::new();
        // Only write the first half of the second block
        {
            let w = ChunkWriter::new(layout, 7, &store);
            let buf = vec![1u8; (layout.block_size / 2) as usize];
            w.write(layout.block_size as u64, &buf).await.unwrap();
        }
        let r = ChunkReader::new(layout, 7, &store);
        // Read from the back half of block 0 to the front half of block 1 (one block total)
        let off = (layout.block_size / 2) as u64;
        let res = r.read(off, layout.block_size as usize).await.unwrap();
        assert_eq!(res.len(), layout.block_size as usize);
        // The first half should be zero-filled and the second half should be ones
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
