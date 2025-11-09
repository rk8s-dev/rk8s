//! ChunkWriter: splits buffered data into block-aligned segments and writes them to the store.

use super::slice::SliceDesc;
use super::store::BlockStore;
use super::{chunk::ChunkLayout, slice::BlockSpan};
use anyhow::Result;

pub struct ChunkWriter<'a, S: BlockStore> {
    layout: ChunkLayout,
    chunk_id: i64,
    store: &'a S,
}

impl<'a, S: BlockStore> ChunkWriter<'a, S> {
    pub fn new(layout: ChunkLayout, chunk_id: i64, store: &'a S) -> Self {
        Self {
            layout,
            chunk_id,
            store,
        }
    }

    /// Split a chunk-local write (offset + buffer) into block writes.
    pub async fn write(&self, offset_in_chunk: u64, buf: &[u8]) -> Result<SliceDesc> {
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
                .write_range((self.chunk_id, sp.block_index), sp.offset_in_block, data)
                .await?;
            cursor += take;
        }
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::chunk::DEFAULT_BLOCK_SIZE;
    use crate::chuck::store::InMemoryBlockStore;

    #[tokio::test]
    async fn test_writer_cross_blocks() {
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let writer = ChunkWriter::new(layout, 1, &store);

        // Write starting from half a block and spanning one and a half blocks.
        let half = (layout.block_size / 2) as usize;
        let len = layout.block_size as usize + half;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate().take(len) {
            *b = (i % 251) as u8; // Nontrivial data pattern
        }
        let slice = writer.write(half as u64, &data).await.unwrap();
        assert_eq!(slice.offset, half as u64);
        assert_eq!(slice.length as usize, len);

        // Read back and verify (reusing read_at).
        let mut out = Vec::with_capacity(len);
        // Back half of the first block.
        out.extend(
            store
                .read_range(
                    (1, 0),
                    DEFAULT_BLOCK_SIZE / 2,
                    (DEFAULT_BLOCK_SIZE / 2) as usize,
                )
                .await
                .unwrap(),
        );
        // Entire second block.
        out.extend(
            store
                .read_range((1, 1), 0, layout.block_size as usize)
                .await
                .unwrap(),
        );

        assert_eq!(out, data);
    }
}
