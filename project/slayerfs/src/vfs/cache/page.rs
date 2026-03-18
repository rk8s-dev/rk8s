use std::io::{Cursor, Read};
use std::mem::take;
use std::sync::Arc;

use crate::chunk::{BlockTag, ChunkSpan, PageTag};
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
    pages: Vec<Option<Page>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CacheSliceStats {
    pub len: u64,
    pub alloc_bytes: u64,
    pub pages_total: usize,
    pub pages_used: usize,
}

impl CacheSlice {
    pub(crate) fn new(config: Arc<WriteConfig>) -> Self {
        assert_eq!(
            config.layout.block_size % config.page_size,
            0,
            "The block size must be a multiple of the page size"
        );

        let (chunk_size, block_size, page_size) = (
            config.layout.chunk_size,
            config.layout.block_size as u64,
            config.page_size as u64,
        );

        let (blocks, pages_per_block) = (
            chunk_size.div_ceil(block_size) as usize,
            block_size.div_ceil(page_size) as usize,
        );

        let pages = vec![None; blocks * pages_per_block];

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

        let (chunk_size, block_size, page_size, pages_per_block) = (
            self.config.layout.chunk_size,
            self.config.layout.block_size as u64,
            self.config.page_size as u64,
            self.pages_per_block(),
        );

        let span = ChunkSpan::new(0, offset, buf.len() as u64);

        for block_span in span.split_into::<BlockTag>(chunk_size, block_size, true) {
            let page_spans = block_span.split_into::<PageTag>(block_size, page_size, true);

            for page_span in page_spans {
                let (block_idx, page_idx) =
                    (block_span.index.as_usize(), page_span.index.as_usize());

                let (flat_idx, end) = (
                    self.flat_index(block_idx, page_idx, pages_per_block),
                    (page_span.offset + page_span.len).as_usize(),
                );

                let slice = {
                    let page = self.ensure_page_mut(flat_idx, page_size as usize);
                    page.write_slice(page_span.offset.as_usize(), end)?
                };

                cursor.read_exact(slice)?;
            }
        }

        Ok(())
    }

    /// Append a buffer behind the latest page. The buffer length must not exceed the remaining length.
    #[tracing::instrument(level = "trace", skip(self, buf), fields(len = buf.len()))]
    pub(crate) fn append(&mut self, buf: &[u8]) -> anyhow::Result<()> {
        let (max_len, next_len) = (self.config.layout.chunk_size, self.len + buf.len() as u64);

        anyhow::ensure!(next_len <= max_len, "append exceeds chunk size");

        let (page_size, mut position, mut cursor) =
            (self.config.page_size as usize, 0usize, Cursor::new(buf));

        while position < buf.len() {
            let (flat_idx, within_page) = self.next_write_slot_flat();

            let read = {
                let page = self.ensure_page_mut(flat_idx, page_size);
                cursor.read(page.write_slice(within_page, page_size)?)?
            };

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

    pub(crate) fn stats(&self) -> CacheSliceStats {
        let pages_used = self.pages.iter().filter(|p| p.is_some()).count();
        CacheSliceStats {
            len: self.len,
            alloc_bytes: self.alloc_bytes,
            pages_total: self.pages.len(),
            pages_used,
        }
    }

    pub(crate) fn freeze(&mut self) {
        for page in self.pages.iter_mut().flatten() {
            page.freeze();
        }
    }

    pub(crate) fn freeze_blocks(&mut self, start: usize, end: usize) {
        let pages_per_block = self.pages_per_block();

        let (start_idx, end_idx) = (start * pages_per_block, end * pages_per_block);

        for page in self.pages[start_idx..end_idx].iter_mut().flatten() {
            page.freeze();
        }
    }

    pub fn release_block(&mut self, idx: Vec<usize>) -> u64 {
        let (page_size, pages_per_block, mut freed) =
            (self.config.page_size as u64, self.pages_per_block(), 0);

        for block_idx in idx {
            let range = self.block_page_range(block_idx, pages_per_block);

            for page in self.pages[range].iter_mut() {
                if page.is_some() {
                    *page = None;
                    freed += page_size;
                }
            }
        }

        self.alloc_bytes = self.alloc_bytes.saturating_sub(freed);
        freed
    }

    pub(crate) fn release_all(&mut self) -> u64 {
        let freed = self.alloc_bytes;
        for page in &mut self.pages {
            *page = None;
        }
        self.alloc_bytes = 0;
        freed
    }

    pub(crate) fn collect_pages(
        &mut self,
        start: usize,
        end: usize,
    ) -> anyhow::Result<Vec<(usize, Vec<Bytes>)>> {
        let (page_size, block_size, pages_per_block) = (
            self.config.page_size as usize,
            self.config.layout.block_size as usize,
            self.pages_per_block(),
        );

        let (mut remaining, skip) = (self.len.as_usize(), start * block_size);
        remaining = remaining.saturating_sub(skip);

        if remaining == 0 {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();

        for block_idx in start..end {
            let mut pages = Vec::new();

            for page_idx in 0..pages_per_block {
                if remaining == 0 {
                    break;
                }

                let (take, flat_idx) = (
                    remaining.min(page_size),
                    self.flat_index(block_idx, page_idx, pages_per_block),
                );

                if let Some(page) = self.pages[flat_idx].as_ref() {
                    let bytes = page.bytes()?;
                    pages.push(bytes.slice(0..take));
                } else {
                    pages.extend(make_zero_bytes(take));
                }

                remaining -= take;
            }

            out.push((block_idx, pages));
        }

        Ok(out)
    }

    fn next_write_slot_flat(&mut self) -> (usize, usize) {
        let (page_size, block_size, pages_per_block, total) = (
            self.config.page_size as usize,
            self.config.layout.block_size as usize,
            self.pages_per_block(),
            self.len.as_usize(),
        );

        let (block_index, within_block) = (total / block_size, total % block_size);

        let (page_index, within_page) = (within_block / page_size, within_block % page_size);

        let flat_idx = self.flat_index(block_index, page_index, pages_per_block);
        (flat_idx, within_page)
    }

    fn pages_per_block(&self) -> usize {
        (self.config.layout.block_size as usize).div_ceil(self.config.page_size as usize)
    }

    fn flat_index(&self, block_idx: usize, page_idx: usize, pages_per_block: usize) -> usize {
        debug_assert_eq!(pages_per_block, self.pages_per_block());
        block_idx * pages_per_block + page_idx
    }

    fn block_page_range(&self, block_idx: usize, pages_per_block: usize) -> std::ops::Range<usize> {
        let (start, end) = (
            block_idx * pages_per_block,
            (block_idx + 1) * pages_per_block,
        );

        start..end
    }

    fn ensure_page_mut(&mut self, flat_idx: usize, page_size: usize) -> &mut Page {
        if self.pages[flat_idx].is_none() {
            self.pages[flat_idx] = Some(Page::new(page_size));
            self.alloc_bytes += page_size as u64;
        }

        self.pages[flat_idx].as_mut().unwrap()
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

    use crate::chunk::ChunkLayout;
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
        let (len, block_size) = (slice.len(), slice.block_size() as u64);

        let end = if len == 0 {
            0
        } else {
            len.div_ceil(block_size) as usize
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
        let (mut slice, data) = (CacheSlice::new(config()), patterned(512, 1));

        slice.append(&data).unwrap();
        slice.freeze();

        assert_eq!(slice.len, data.len() as u64);
        assert_eq!(slice.pages.iter().filter(|p| p.is_some()).count(), 1);
        assert_eq!(collect_all(&mut slice), data);
    }

    #[test]
    fn test_append_spans_pages() {
        let (mut slice, data) = (CacheSlice::new(config()), patterned(1024 + 10, 3));

        slice.append(&data).unwrap();
        slice.freeze();

        assert_eq!(slice.len, data.len() as u64);
        assert_eq!(slice.pages.iter().filter(|p| p.is_some()).count(), 2);
        assert_eq!(collect_all(&mut slice), data);
    }

    #[test]
    fn test_append_spans_blocks() {
        let (mut slice, data) = (CacheSlice::new(config()), patterned(4 * 1024 + 512, 7));

        slice.append(&data).unwrap();
        slice.freeze();

        let (block_size, page_size) = (
            slice.config.layout.block_size as usize,
            slice.config.page_size as usize,
        );

        let pages_per_block = block_size.div_ceil(page_size);
        assert_eq!(
            slice.pages.iter().filter(|p| p.is_some()).count(),
            pages_per_block + 1
        );
        assert_eq!(collect_all(&mut slice), data);
    }

    #[test]
    fn test_append_multiple_calls() {
        let (mut slice, first, second) = (
            CacheSlice::new(config()),
            patterned(600, 11),
            patterned(900, 23),
        );

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
        let (mut slice, first, second) = (
            CacheSlice::new(config()),
            patterned(600, 11),
            patterned(600, 23),
        );

        slice.append(&first).unwrap();
        slice.write_at(0, &second).unwrap();
        slice.freeze();

        let expected = second.clone();

        assert_eq!(slice.len, expected.len() as u64);
        assert_eq!(collect_all(&mut slice), expected);
    }

    #[test]
    fn test_write_at_overwrite_middle() {
        let (mut slice, data, patch, offset) = (
            CacheSlice::new(config()),
            patterned(1500, 5),
            patterned(300, 200),
            700usize,
        );

        slice.append(&data).unwrap();
        slice.write_at(offset as u64, &patch).unwrap();
        slice.freeze();

        let mut expected = data.clone();

        let (start, end) = (offset, offset + patch.len());

        expected[start..end].copy_from_slice(&patch);

        assert_eq!(slice.len, data.len() as u64);
        assert_eq!(collect_all(&mut slice), expected);
    }

    #[test]
    fn test_write_at_crosses_block_boundary() {
        let (mut slice, data, patch, offset) = (
            CacheSlice::new(config()),
            patterned(4 * 1024 + 512, 9),
            patterned(512, 77),
            4 * 1024 - 256,
        );

        slice.append(&data).unwrap();
        slice.write_at(offset as u64, &patch).unwrap();
        slice.freeze();

        let mut expected = data.clone();

        let (start, end) = (offset, offset + patch.len());

        expected[start..end].copy_from_slice(&patch);

        assert_eq!(slice.len, data.len() as u64);
        assert_eq!(collect_all(&mut slice), expected);
    }

    #[test]
    fn test_can_write_at_disallows_append_or_extend() {
        let (mut slice, data) = (CacheSlice::new(config()), patterned(1024, 17));

        slice.append(&data).unwrap();

        assert!(slice.can_write_at(0, 512));
        assert!(slice.can_write_at(512, 512));
        assert!(!slice.can_write_at(1024, 1));
        assert!(!slice.can_write_at(900, 200));
    }

    #[test]
    fn test_append_then_write_at_overwrite_full() {
        let total = 16 * 1024;

        let (mut slice, first, second) = (
            CacheSlice::new(config()),
            patterned(6000, 1),
            patterned(total - 6000, 2),
        );

        slice.append(&first).unwrap();
        slice.append(&second).unwrap();

        let overwrite = patterned(total, 9);
        slice.write_at(0, &overwrite).unwrap();
        slice.freeze();

        assert_eq!(slice.len, total as u64);
        assert_eq!(collect_all(&mut slice), overwrite);
    }
}
