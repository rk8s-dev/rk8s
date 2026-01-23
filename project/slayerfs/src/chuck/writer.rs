//! DataUploader: writes a slice payload into blocks without touching metadata.

use super::chunk::ChunkLayout;
use super::slice::{SliceDesc, block_span_iter};
use super::store::BlockStore;
use crate::meta::MetaStore;
use crate::vfs::backend::Backend;
use anyhow::Result;
use futures_util::future::join_all;

pub(crate) struct DataUploader<'a, B, M> {
    layout: ChunkLayout,
    id: u64,
    backend: &'a Backend<B, M>,
}

impl<'a, B, M> DataUploader<'a, B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    pub(crate) fn new(layout: ChunkLayout, id: u64, backend: &'a Backend<B, M>) -> Self {
        Self {
            layout,
            id,
            backend,
        }
    }

    /// Only write the data of a slice into the object storage. Callers must update metadata.
    pub(crate) async fn write_at(
        &self,
        slice_id: u64,
        offset: u32,
        buf: &[u8],
    ) -> Result<SliceDesc> {
        let desc = SliceDesc {
            slice_id,
            chunk_id: self.id,
            offset,
            length: buf.len() as u32,
        };

        let mut cursor = 0;
        let mut futures = Vec::new();

        for span in block_span_iter(desc, self.layout) {
            let take = span.len as usize;
            let data = &buf[cursor..(cursor + take)];

            let future =
                self.backend
                    .store()
                    .write_range((slice_id, span.index as u32), span.offset, data);
            futures.push(future);
            cursor += take;
        }

        for res in join_all(futures).await {
            res?;
        }
        Ok(desc)
    }

    /// Write slice data using vectored I/O to avoid intermediate copying.
    /// Accepts scattered page buffers and writes them efficiently.
    /// Note: This currently consolidates pages per block span but eliminates
    /// the initial full-slice consolidation, reducing one copy operation.
    pub(crate) async fn write_at_vectored(
        &self,
        slice_id: u64,
        offset: u32,
        pages: &[Vec<u8>],
    ) -> Result<SliceDesc> {
        let total_len: u32 = pages.iter().map(|p| p.len() as u32).sum();
        let desc = SliceDesc {
            slice_id,
            chunk_id: self.id,
            offset,
            length: total_len,
        };

        // Collect all span data first
        let mut spans_data = Vec::new();
        let mut page_idx = 0;
        let mut page_offset = 0;

        for span in block_span_iter(desc, self.layout) {
            let mut span_data = Vec::with_capacity(span.len as usize);
            let mut remaining = span.len as usize;

            // Collect page data that contributes to this block span
            while remaining > 0 && page_idx < pages.len() {
                let page = &pages[page_idx];
                let available = page.len() - page_offset;
                let take = remaining.min(available);
                
                span_data.extend_from_slice(&page[page_offset..page_offset + take]);
                
                remaining -= take;
                page_offset += take;
                
                if page_offset >= page.len() {
                    page_idx += 1;
                    page_offset = 0;
                }
            }

            spans_data.push((span.index, span.offset, span_data));
        }

        // Write all spans in parallel - now spans_data lives for the whole function
        let mut futures = Vec::new();
        for (index, offset, data) in &spans_data {
            let future = self.backend.store().write_range(
                (slice_id, *index as u32),
                *offset,
                data.as_slice(),
            );
            futures.push(future);
        }

        for res in join_all(futures).await {
            res?;
        }
        Ok(desc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::chunk::ChunkLayout;
    use crate::chuck::reader::DataFetcher;
    use crate::chuck::store::InMemoryBlockStore;
    use crate::meta::SLICE_ID_KEY;
    use crate::meta::factory::create_meta_store_from_url;
    use crate::vfs::backend::Backend;
    use std::sync::Arc;

    fn small_layout() -> ChunkLayout {
        ChunkLayout {
            chunk_size: 16 * 1024,
            block_size: 4 * 1024,
        }
    }

    fn patterned(len: usize, seed: u8) -> Vec<u8> {
        let mut buf = vec![0u8; len];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        buf
    }

    #[tokio::test]
    async fn test_data_uploader_roundtrip() {
        let layout = small_layout();
        let store = Arc::new(InMemoryBlockStore::new());
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));

        let data = patterned(layout.block_size as usize + 512, 7);
        let offset = 512u32;
        let slice_id = meta.next_id(SLICE_ID_KEY).await.unwrap();

        let uploader = DataUploader::new(layout, 1, backend.as_ref());
        let desc = uploader
            .write_at(slice_id as u64, offset, &data)
            .await
            .unwrap();
        meta.append_slice(1, desc).await.unwrap();

        let mut fetcher = DataFetcher::new(layout, 1, backend.as_ref());
        fetcher.prepare_slices().await.unwrap();
        let out = fetcher.read_at(offset, data.len()).await.unwrap();
        assert_eq!(out, data);
    }
}
