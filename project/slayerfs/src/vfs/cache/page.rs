use std::io::{Cursor, Read};
use std::sync::Arc;

use bytes::BytesMut;
use derive_more::{Deref, DerefMut};

use crate::vfs::config::WriteConfig;

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
    pub(crate) fn append(&mut self, buf: &[u8]) -> anyhow::Result<()> {
        let max_len = self.config.layout.chunk_size;
        let next_len = self.len as u64 + buf.len() as u64;
        anyhow::ensure!(next_len <= max_len, "append exceeds chunk size");

        let mut position = 0;
        let mut cursor = Cursor::new(buf);

        while position < buf.len() {
            let next_page = self.next_write_slice();
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

    pub(crate) fn collect_pages(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.len as usize);
        let mut remaining = self.len as usize;
        for block in &self.pages {
            for page in block {
                if remaining == 0 {
                    return buf;
                }
                let take = remaining.min(page.data.len());
                buf.extend_from_slice(&page.data[..take]);
                remaining -= take;
            }
        }
        buf
    }

    /// Acquire the next slice that can write into.
    /// The returned slice is empty, belongs to a single page and never overlay.
    fn next_write_slice(&mut self) -> &mut [u8] {
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

        let page = &mut self.pages[block_index][page_index];
        &mut page[within_page..page_size]
    }
}

#[derive(Deref, DerefMut, Default, Clone)]
pub(crate) struct Page {
    #[deref]
    data: BytesMut,
}

impl Page {
    pub(crate) fn new(size: usize) -> Self {
        Self {
            data: BytesMut::zeroed(size),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::chuck::ChunkLayout;
    use crate::vfs::cache::page::CacheSlice;
    use crate::vfs::config::WriteConfig;

    fn config() -> Arc<WriteConfig> {
        Arc::new(WriteConfig::new(
            ChunkLayout {
                chunk_size: 16 * 1024,
                block_size: 4 * 1024,
            },
            1024,
        ))
    }

    fn patterned(len: usize, seed: u8) -> Vec<u8> {
        let mut buf = vec![0u8; len];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        buf
    }

    #[test]
    fn test_append_single_page() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(512, 1);
        slice.append(&data).unwrap();

        assert_eq!(slice.len, data.len() as u32);
        assert_eq!(slice.pages[0].len(), 1);
        assert_eq!(slice.collect_pages(), data);
    }

    #[test]
    fn test_append_spans_pages() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(1024 + 10, 3);
        slice.append(&data).unwrap();

        assert_eq!(slice.len, data.len() as u32);
        assert_eq!(slice.pages[0].len(), 2);
        assert_eq!(slice.collect_pages(), data);
    }

    #[test]
    fn test_append_spans_blocks() {
        let mut slice = CacheSlice::new(config());
        let data = patterned(4 * 1024 + 512, 7);
        slice.append(&data).unwrap();

        let pages_per_block = 4;
        assert_eq!(slice.pages[0].len(), pages_per_block);
        assert_eq!(slice.pages[1].len(), 1);
        assert_eq!(slice.collect_pages(), data);
    }

    #[test]
    fn test_append_multiple_calls() {
        let mut slice = CacheSlice::new(config());
        let first = patterned(600, 11);
        let second = patterned(900, 23);

        slice.append(&first).unwrap();
        slice.append(&second).unwrap();

        let mut expected = first.clone();
        expected.extend_from_slice(&second);

        assert_eq!(slice.len, expected.len() as u32);
        assert_eq!(slice.collect_pages(), expected);
    }
}
