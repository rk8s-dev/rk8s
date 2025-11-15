//! ChunkWriter: splits buffered data into block-aligned segments and writes them to the store.

use super::slice::{SliceDesc, SliceIO, Write};
use super::store::BlockStore;
use super::{chunk::ChunkLayout, slice::BlockSpan};
use crate::meta::{MetaStore, SLICE_ID_KEY};
use anyhow::Result;
use std::sync::{Arc, Mutex};

pub struct ChunkWriter<'a, B, S> {
    layout: ChunkLayout,
    chunk_id: u64,
    store: &'a B,
    meta: &'a S,
}

impl<'a, B: BlockStore, S: MetaStore> ChunkWriter<'a, B, S> {
    pub fn new(layout: ChunkLayout, chunk_id: u64, store: &'a B, meta: &'a S) -> Self {
        Self {
            layout,
            chunk_id,
            store,
            meta,
        }
    }

    /// Split a chunk-local write (offset + buffer) into block writes.
    pub async fn write(&self, offset: u32, buf: &[u8]) -> Result<()> {
        let slice_id = self.meta.next_id(SLICE_ID_KEY).await?;
        let slice = SliceDesc {
            slice_id: slice_id as u64,
            chunk_id: self.chunk_id,
            offset,
            length: buf.len() as u32,
        };

        let writer = SliceIO::<Write, _>::new(slice, self.layout, self.store);

        let desc = writer.write(buf).await?;
        self.meta.append_slice(self.chunk_id, desc).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::ChunkReader;
    use crate::chuck::chunk::DEFAULT_BLOCK_SIZE;
    use crate::chuck::store::InMemoryBlockStore;
    use crate::meta::factory::create_meta_store_from_url;

    #[tokio::test]
    async fn test_writer_cross_blocks() {
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let writer = ChunkWriter::new(layout, 1, &store, &meta);

        // Write starting from half a block and spanning one and a half blocks.
        let half = (layout.block_size / 2) as usize;
        let len = layout.block_size as usize + half;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate().take(len) {
            *b = (i % 251) as u8; // Nontrivial data pattern
        }
        writer.write(half as u32, &data).await.unwrap();

        // Read back and verify (reusing read_at).
        let mut out = Vec::with_capacity(len);
        // Back half of the first block.
        let mut first = vec![0u8; (DEFAULT_BLOCK_SIZE / 2) as usize];
        store
            .read_range((1, 0), DEFAULT_BLOCK_SIZE / 2, &mut first)
            .await
            .unwrap();
        out.extend_from_slice(&first);
        // Entire second block.
        let mut second = vec![0u8; layout.block_size as usize];
        store.read_range((1, 1), 0, &mut second).await.unwrap();
        out.extend_from_slice(&second);

        assert_eq!(out, data);
    }

    #[tokio::test]
    async fn test_writer_copy_on_write_appends_slice() {
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let writer = ChunkWriter::new(layout, 9, &store, &meta);

        let half = (layout.block_size / 2) as usize;
        let first = vec![1u8; half];
        writer.write(0, &first).await.unwrap();

        let second = vec![2u8; half];
        writer.write(0, &second).await.unwrap();

        let slices = meta.get_slices(9).await.unwrap();
        assert_eq!(slices.len(), 2, "copy-on-write must append slices");
        assert_eq!(slices[0].chunk_id, slices[1].chunk_id);
        assert_eq!(slices[1].chunk_id, 9);
        assert_eq!(slices[0].offset, slices[1].offset);
        assert_eq!(slices[1].offset, 0);
        assert_eq!(slices[0].length, slices[1].length);
        assert_eq!(slices[0].length, half as u32);

        let mut reader = ChunkReader::new(layout, 9, &store, &meta);
        reader.prepare_slices().await.unwrap();
        let out = reader.read(0, half).await.unwrap();
        assert!(out.iter().all(|&b| b == 2));
    }
}
