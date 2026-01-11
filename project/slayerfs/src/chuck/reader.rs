//! DataFetcher: fetch data from blocks according to offset/length, handling gaps with zeros.

use super::chunk::ChunkLayout;
use super::slice::{SliceDesc, block_span_iter};
use super::store::BlockStore;
use crate::meta::MetaStore;
use crate::utils::Intervals;
use crate::vfs::backend::Backend;
use anyhow::{Result, ensure};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use std::cmp::{max, min};

pub(crate) struct DataFetcher<'a, B, M> {
    layout: ChunkLayout,
    id: u64,
    slices: Vec<SliceDesc>,
    prepared: bool,
    backend: &'a Backend<B, M>,
}

impl<'a, B, M> DataFetcher<'a, B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    pub(crate) fn new(layout: ChunkLayout, id: u64, backend: &'a Backend<B, M>) -> Self {
        Self {
            layout,
            id,
            backend,
            prepared: false,
            slices: Vec::new(),
        }
    }

    pub(crate) async fn prepare_slices(&mut self) -> anyhow::Result<()> {
        self.slices = self.backend.meta().get_slices(self.id).await?;
        self.prepared = true;
        Ok(())
    }

    pub(crate) async fn read_at(&mut self, offset: u32, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }
        ensure!(
            self.prepared,
            "DataFetcher::read_at requires prepare_slices() to run first"
        );

        let mut buf = vec![0; len];

        let mut intervals = Intervals::new(offset, offset + len as u32);
        let mut need_read = Vec::new();
        for slice in self.slices.iter().copied().rev() {
            for (l, r) in intervals.cut(slice.offset, slice.offset + slice.length) {
                need_read.push((l, r, slice));
            }
        }
        need_read.sort_by_key(|(l, _, _)| *l);

        let layout = self.layout;
        let backend = self.backend;

        {
            let mut cursor = 0;
            let mut tail = &mut buf[..];
            let mut futures = FuturesUnordered::new();

            for (l, r, slice) in need_read {
                let start = (l - offset) as usize;
                let len = (r - l) as usize;
                debug_assert!(start >= cursor);

                // Skip gaps
                let gap = start - cursor;
                let (_, rest) = tail.split_at_mut(gap);
                let (seg, rest) = rest.split_at_mut(len);
                tail = rest;
                cursor = start + len;

                let desc = SliceDesc {
                    offset: l,
                    length: r - l,
                    ..slice
                };

                futures.push(async move {
                    let mut pos = 0_usize;
                    for span in block_span_iter(desc, layout) {
                        let take = span.len as usize;
                        let out = &mut seg[pos..pos + take];
                        backend
                            .store()
                            .read_range((desc.slice_id, span.index as u32), span.offset, out)
                            .await?;
                        pos += take;
                    }
                    Ok::<_, anyhow::Error>(())
                });
            }

            while let Some(res) = futures.next().await {
                res?;
            }
        }
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::store::InMemoryBlockStore;
    use crate::chuck::writer::DataUploader;
    use crate::meta::SLICE_ID_KEY;
    use crate::meta::factory::create_meta_store_from_url;
    use crate::vfs::backend::Backend;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_reader_zero_fills_holes() {
        let layout = ChunkLayout::default();
        let store = Arc::new(InMemoryBlockStore::new());
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));
        // Only write the first half of the second block
        {
            let buf = vec![1u8; (layout.block_size / 2) as usize];
            let slice_id = meta.next_id(SLICE_ID_KEY).await.unwrap();
            let uploader = DataUploader::new(layout, 7, backend.as_ref());
            let desc = uploader
                .write_at(slice_id as u64, layout.block_size, &buf)
                .await
                .unwrap();
            meta.append_slice(7, desc).await.unwrap();
        }
        let mut r = DataFetcher::new(layout, 7, backend.as_ref());
        r.prepare_slices().await.unwrap();
        // Read from the back half of block 0 to the front half of block 1 (one block total)
        let off = layout.block_size / 2;
        let res = r.read_at(off, layout.block_size as usize).await.unwrap();
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

    #[tokio::test]
    async fn test_fetcher_cross_block_boundary() {
        let layout = ChunkLayout {
            chunk_size: 16 * 1024,
            block_size: 4 * 1024,
        };
        let store = Arc::new(InMemoryBlockStore::new());
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));

        let offset = layout.block_size - 512;
        let data = vec![7u8; 2048];
        let slice_id = meta.next_id(SLICE_ID_KEY).await.unwrap();
        let uploader = DataUploader::new(layout, 3, backend.as_ref());
        let desc = uploader
            .write_at(slice_id as u64, offset, &data)
            .await
            .unwrap();
        meta.append_slice(3, desc).await.unwrap();

        let mut fetcher = DataFetcher::new(layout, 3, backend.as_ref());
        fetcher.prepare_slices().await.unwrap();
        let out = fetcher.read_at(offset, data.len()).await.unwrap();
        assert_eq!(out, data);
    }

    #[tokio::test]
    async fn test_fetcher_overlapping_slices_latest_wins() {
        let layout = ChunkLayout {
            chunk_size: 16 * 1024,
            block_size: 4 * 1024,
        };
        let store = Arc::new(InMemoryBlockStore::new());
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));

        let data1 = vec![1u8; 2048];
        let data2 = vec![2u8; 2048];

        let slice_id1 = meta.next_id(SLICE_ID_KEY).await.unwrap();
        let uploader = DataUploader::new(layout, 9, backend.as_ref());
        let desc1 = uploader
            .write_at(slice_id1 as u64, 0, &data1)
            .await
            .unwrap();
        meta.append_slice(9, desc1).await.unwrap();

        let slice_id2 = meta.next_id(SLICE_ID_KEY).await.unwrap();
        let desc2 = uploader
            .write_at(slice_id2 as u64, 1024, &data2)
            .await
            .unwrap();
        meta.append_slice(9, desc2).await.unwrap();

        let mut fetcher = DataFetcher::new(layout, 9, backend.as_ref());
        fetcher.prepare_slices().await.unwrap();
        let out = fetcher.read_at(0, 3072).await.unwrap();
        assert_eq!(&out[..1024], &data1[..1024]);
        assert_eq!(&out[1024..], &data2[..]);
    }
}
