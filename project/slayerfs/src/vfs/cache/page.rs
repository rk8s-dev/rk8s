use std::io::{Cursor, Read};
use std::mem::take;
use std::sync::Arc;

use crate::chuck::{BlockTag, ChunkSpan, PageTag};
use crate::utils::NumCastExt;
use crate::utils::zero::make_zero_bytes;
use crate::vfs::config::WriteConfig;
use bytes::{Bytes, BytesMut};

#[derive(Debug)]
pub(crate) enum WriteAction {
    Overlap,
    Append,
}

pub(crate) struct CacheSlice {
    config: Arc<WriteConfig>,
    len: u64,
    alloc_bytes: u64,
    pages: Vec<Vec<Page>>,
}

impl CacheSlice {
    pub(crate) fn new(config: Arc<WriteConfig>) -> Self {
        assert_eq!(
            config.layout.block_size % config.page_size,
            0,
            "The block size must be a multiple of the page size"
        );

        let blocks = config
            .layout
            .chunk_size
            .div_ceil(config.layout.block_size as u64) as usize;
        let pages = vec![Vec::new(); blocks];
        Self {
            config,
            len: 0,
            alloc_bytes: 0,
            pages,
        }
    }

    pub(crate) fn can_write(&self, offset: u64, len: u64) -> Option<WriteAction> {
        if self.can_append(offset) {
            return Some(WriteAction::Append);
        }

        if self.can_write_at(offset, len) {
            return Some(WriteAction::Overlap);
        }

        None
    }

    pub(crate) fn write(
        &mut self,
        offset: u64,
        buf: &[u8],
        action: WriteAction,
    ) -> anyhow::Result<()> {
        match action {
            WriteAction::Append => self.append(buf),
            WriteAction::Overlap => self.write_at(offset, buf),
        }
    }

    pub(crate) fn can_write_at(&self, offset: u64, len: u64) -> bool {
        let end = offset.saturating_add(len);
        end <= self.len
    }

    /// Use buffer to overlap specific range.
    #[tracing::instrument(level = "trace", skip(self, buf), fields(len = buf.len()))]
    pub(crate) fn write_at(&mut self, offset: u64, buf: &[u8]) -> anyhow::Result<()> {
        let mut cursor = Cursor::new(buf);

        let chunk_size = self.config.layout.chunk_size;
        let block_size = self.config.layout.block_size;
        let page_size = self.config.page_size;

        let span = ChunkSpan::new(0, offset, buf.len() as u64);
        for block_span in span.split_into::<BlockTag>(chunk_size, block_size as u64, true) {
            let span = block_span.split_into::<PageTag>(block_size as u64, page_size as u64, true);

            for page_span in span {
                let page = &mut self.pages[block_span.index.as_usize()][page_span.index.as_usize()];
                let end = (page_span.offset + page_span.len).as_usize();

                let slice = page.write_slice(page_span.offset.as_usize(), end)?;
                cursor.read_exact(slice)?;
            }
        }

        Ok(())
    }

    /// Append a buffer behind the latest page. The buffer length must not exceed the remaining length.
    #[tracing::instrument(level = "trace", skip(self, buf), fields(len = buf.len()))]
    pub(crate) fn append(&mut self, buf: &[u8]) -> anyhow::Result<()> {
        let max_len = self.config.layout.chunk_size;
        let next_len = self.len + buf.len() as u64;
        anyhow::ensure!(next_len <= max_len, "append exceeds chunk size");

        let page_size = self.config.page_size as usize;
        let mut position = 0;
        let mut cursor = Cursor::new(buf);

        while position < buf.len() {
            let (block_index, page_index, within_page) = self.next_write_slot();
            let page = &mut self.pages[block_index][page_index];
            let next_page = page.write_slice(within_page, page_size)?;
            let read = cursor.read(next_page)?;

            if read == 0 {
                break;
            }

            position += read;
            self.len += read as u64;
        }
        Ok(())
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    pub(crate) fn block_size(&self) -> u32 {
        self.config.layout.block_size
    }

    pub(crate) fn can_append(&self, offset: u64) -> bool {
        self.len == offset
    }

    pub(crate) fn freeze(&mut self) {
        for block in &mut self.pages {
            for page in block {
                page.freeze();
            }
        }
    }

    pub(crate) fn freeze_blocks(&mut self, start: usize, end: usize) {
        for block in self.pages[start..end].iter_mut() {
            for page in block.iter_mut() {
                page.freeze();
            }
        }
    }

    pub fn release_block(&mut self, idx: Vec<usize>) -> u64 {
        let page_size = self.config.page_size as u64;
        let mut freed = 0;

        for index in idx {
            let pages = self.pages[index].len() as u64;
            freed += pages * page_size;
            self.pages[index].clear();
        }

        self.alloc_bytes = self.alloc_bytes.saturating_sub(freed);
        freed
    }

    pub(crate) fn release_all(&mut self) -> u64 {
        let freed = self.alloc_bytes;

        for block in &mut self.pages {
            block.clear();
        }

        self.alloc_bytes = 0;
        freed
    }

    pub(crate) fn collect_pages(
        &mut self,
        start: usize,
        end: usize,
    ) -> anyhow::Result<Vec<(usize, Vec<Bytes>)>> {
        let page_size = self.config.page_size as usize;
        let block_size = self.config.layout.block_size as usize;
        let pages_per_block = block_size / page_size;

        let mut remaining = self.len.as_usize();
        let skip = start * block_size;
        remaining = remaining.saturating_sub(skip);

        if remaining == 0 {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();

        for idx in start..end {
            let block = &self.pages[idx];
            let mut pages = Vec::new();

            for page_idx in 0..pages_per_block {
                if remaining == 0 {
                    break;
                }

                let take = remaining.min(page_size);
                if let Some(page) = block.get(page_idx) {
                    let bytes = page.bytes()?;
                    pages.push(bytes.slice(0..take));
                } else {
                    pages.extend(make_zero_bytes(take));
                }

                remaining -= take;
            }

            out.push((idx, pages));
        }

        Ok(out)
    }

    /// Acquire the next writable slot (block, page, offset).
    /// The returned slot belongs to a single page and never overlaps existing writes.
    fn next_write_slot(&mut self) -> (usize, usize, usize) {
        let page_size = self.config.page_size as usize;
        let block_size = self.config.layout.block_size as usize;

        let total = self.len.as_usize();
        let block_index = total / block_size;
        let within_block = total % block_size;
        let page_index = within_block / page_size;
        let within_page = within_block % page_size;

        // Allocate a complete page
        if self.pages[block_index].len() <= page_index {
            self.pages[block_index].push(Page::new(page_size));
            self.alloc_bytes += page_size as u64;
        }
        (block_index, page_index, within_page)
    }

    pub(crate) fn alloc_bytes(&self) -> u64 {
        self.alloc_bytes
    }
}

#[derive(Clone)]
enum PageBuf {
    Mutable(BytesMut),
    Frozen(Bytes),
}

#[derive(Clone)]
pub(crate) struct Page {
    data: PageBuf,
}

impl Page {
    pub(crate) fn new(size: usize) -> Self {
        let buf = BytesMut::zeroed(size);

        Self {
            data: PageBuf::Mutable(buf),
        }
    }

    fn write_slice(&mut self, start: usize, end: usize) -> anyhow::Result<&mut [u8]> {
        match &mut self.data {
            PageBuf::Mutable(buf) => Ok(&mut buf[start..end]),
            // Return an error: a frozen page means the caller raced with a freeze.
            PageBuf::Frozen(_) => {
                anyhow::bail!("attempt to write to frozen page");
            }
        }
    }

    fn freeze(&mut self) {
        if let PageBuf::Mutable(buf) = &mut self.data {
            let frozen = take(buf).freeze();
            self.data = PageBuf::Frozen(frozen);
        }
    }

    fn bytes(&self) -> anyhow::Result<Bytes> {
        match &self.data {
            PageBuf::Frozen(buf) => Ok(buf.clone()),
            PageBuf::Mutable(_) => {
                // Return an error instead of panicking to surface races with freeze.
                anyhow::bail!(
                    "collect_pages called on mutable page (would read uninitialized bytes)"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::chuck::ChunkLayout;
    use crate::vfs::cache::page::CacheSlice;
    use crate::vfs::config::WriteConfig;
    use bytes::Bytes;

    fn config() -> Arc<WriteConfig> {
        Arc::new(
            WriteConfig::new(ChunkLayout {
                chunk_size: 16 * 1024,
                block_size: 4 * 1024,
            })
            .page_size(1024),
        )
    }

    fn patterned(len: usize, seed: u8) -> Vec<u8> {
        let mut buf = vec![0u8; len];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        buf
    }

    fn flatten(parts: Vec<Bytes>) -> Vec<u8> {
        parts.into_iter().flat_map(|b| b.to_vec()).collect()
    }

    fn collect_all(slice: &mut CacheSlice) -> Vec<u8> {
        let len = slice.len();
        let block_size = slice.block_size() as u64;
        let end = if len == 0 {
            0
        } else {
            ((len + block_size - 1) / block_size) as usize
        };
        let mut blocks = slice.collect_pages(0, end).unwrap();
        blocks.sort_by_key(|(idx, _)| *idx);
        let mut out = Vec::new();
        for (_, pages) in blocks {
            out.extend(flatten(pages));
        }
        out
    }

    #[test]
    fn test_append_single_page() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(512, 1);
        slice.append(&data).unwrap();
        slice.freeze();

        assert_eq!(slice.len, data.len() as u64);
        assert_eq!(slice.pages[0].len(), 1);
        assert_eq!(collect_all(&mut slice), data);
    }

    #[test]
    fn test_append_spans_pages() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(1024 + 10, 3);
        slice.append(&data).unwrap();
        slice.freeze();

        assert_eq!(slice.len, data.len() as u64);
        assert_eq!(slice.pages[0].len(), 2);
        assert_eq!(collect_all(&mut slice), data);
    }

    #[test]
    fn test_append_spans_blocks() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(4 * 1024 + 512, 7);
        slice.append(&data).unwrap();
        slice.freeze();

        let pages_per_block = 4;
        assert_eq!(slice.pages[0].len(), pages_per_block);
        assert_eq!(slice.pages[1].len(), 1);
        assert_eq!(collect_all(&mut slice), data);
    }

    #[test]
    fn test_append_multiple_calls() {
        let mut slice = CacheSlice::new(config());
        let first = patterned(600, 11);
        let second = patterned(900, 23);

        slice.append(&first).unwrap();
        slice.append(&second).unwrap();
        slice.freeze();

        let mut expected = first.clone();
        expected.extend_from_slice(&second);

        assert_eq!(slice.len, expected.len() as u64);
        assert_eq!(collect_all(&mut slice), expected);
    }

    #[test]
    fn test_write_at() {
        let mut slice = CacheSlice::new(config());
        let first = patterned(600, 11);
        let second = patterned(600, 23);

        slice.append(&first).unwrap();
        slice.write_at(0, &second).unwrap();
        slice.freeze();

        let expected = second.clone();

        assert_eq!(slice.len, expected.len() as u64);
        assert_eq!(collect_all(&mut slice), expected);
    }

    #[test]
    fn test_write_at_overwrite_middle() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(1500, 5);
        let patch = patterned(300, 200);
        let offset = 700usize;

        slice.append(&data).unwrap();
        slice.write_at(offset as u64, &patch).unwrap();
        slice.freeze();

        let mut expected = data.clone();
        let start = offset as usize;
        let end = start + patch.len();
        expected[start..end].copy_from_slice(&patch);

        assert_eq!(slice.len, data.len() as u64);
        assert_eq!(collect_all(&mut slice), expected);
    }

    #[test]
    fn test_write_at_crosses_block_boundary() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(4 * 1024 + 512, 9);
        let patch = patterned(512, 77);
        let offset = 4 * 1024 - 256;

        slice.append(&data).unwrap();
        slice.write_at(offset as u64, &patch).unwrap();
        slice.freeze();

        let mut expected = data.clone();
        let start = offset;
        let end = start + patch.len();
        expected[start..end].copy_from_slice(&patch);

        assert_eq!(slice.len, data.len() as u64);
        assert_eq!(collect_all(&mut slice), expected);
    }

    #[test]
    fn test_can_write_at_disallows_append_or_extend() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(1024, 17);

        slice.append(&data).unwrap();

        assert!(slice.can_write_at(0, 512));
        assert!(slice.can_write_at(512, 512));
        assert!(!slice.can_write_at(1024, 1));
        assert!(!slice.can_write_at(900, 200));
    }

    #[test]
    fn test_append_then_write_at_overwrite_full() {
        let mut slice = CacheSlice::new(config());
        let total = 16 * 1024;
        let first = patterned(6000, 1);
        let second = patterned(total - first.len(), 2);

        slice.append(&first).unwrap();
        slice.append(&second).unwrap();

        let overwrite = patterned(total, 9);
        slice.write_at(0, &overwrite).unwrap();
        slice.freeze();

        assert_eq!(slice.len, total as u64);
        assert_eq!(collect_all(&mut slice), overwrite);
    }
}
