use std::io::{Cursor, Read};
use std::mem::take;
use std::sync::Arc;

use crate::utils::zero::make_zero_bytes;
use crate::vfs::config::WriteConfig;
use bytes::{Bytes, BytesMut};

pub(crate) struct CacheSlice {
    config: Arc<WriteConfig>,
    len: u32,
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
            pages,
        }
    }

    /// Append a buffer behind the latest page. The buffer length must not exceed the remaining length.
    #[tracing::instrument(level = "trace", skip(self, buf), fields(len = buf.len()))]
    pub(crate) fn append(&mut self, buf: &[u8]) -> anyhow::Result<()> {
        let max_len = self.config.layout.chunk_size;
        let next_len = self.len as u64 + buf.len() as u64;
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
            self.len += read as u32;
        }
        Ok(())
    }

    pub(crate) fn len(&self) -> u32 {
        self.len
    }

    pub(crate) fn can_append(&self, offset: u32) -> bool {
        self.len == offset
    }

    pub(crate) fn freeze(&mut self) {
        for block in &mut self.pages {
            for page in block {
                page.freeze();
            }
        }
    }

    pub(crate) fn collect_pages(&self) -> anyhow::Result<Vec<Bytes>> {
        let page_size = self.config.page_size as usize;
        let block_size = self.config.layout.block_size as usize;
        let pages_per_block = block_size / page_size;

        let mut remaining = self.len as usize;
        let mut out = Vec::new();

        for block in &self.pages {
            for page_idx in 0..pages_per_block {
                if remaining == 0 {
                    return Ok(out);
                }

                let take = remaining.min(page_size);
                if let Some(page) = block.get(page_idx) {
                    let bytes = page.bytes()?;
                    out.push(bytes.slice(0..take));
                } else {
                    out.extend(make_zero_bytes(take));
                }

                remaining -= take;
            }
        }
        Ok(out)
    }

    /// Acquire the next writable slot (block, page, offset).
    /// The returned slot belongs to a single page and never overlaps existing writes.
    fn next_write_slot(&mut self) -> (usize, usize, usize) {
        let page_size = self.config.page_size as usize;
        let block_size = self.config.layout.block_size as usize;

        let total = self.len as usize;
        let block_index = total / block_size;
        let within_block = total % block_size;
        let page_index = within_block / page_size;
        let within_page = within_block % page_size;

        // Allocate a complete page
        if self.pages[block_index].len() <= page_index {
            self.pages[block_index].push(Page::new(page_size));
        }
        (block_index, page_index, within_page)
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

    #[test]
    fn test_append_single_page() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(512, 1);
        slice.append(&data).unwrap();
        slice.freeze();

        assert_eq!(slice.len, data.len() as u32);
        assert_eq!(slice.pages[0].len(), 1);
        assert_eq!(flatten(slice.collect_pages().unwrap()), data);
    }

    #[test]
    fn test_append_spans_pages() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(1024 + 10, 3);
        slice.append(&data).unwrap();
        slice.freeze();

        assert_eq!(slice.len, data.len() as u32);
        assert_eq!(slice.pages[0].len(), 2);
        assert_eq!(flatten(slice.collect_pages().unwrap()), data);
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
        assert_eq!(flatten(slice.collect_pages().unwrap()), data);
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

        assert_eq!(slice.len, expected.len() as u32);
        assert_eq!(flatten(slice.collect_pages().unwrap()), expected);
    }
}
