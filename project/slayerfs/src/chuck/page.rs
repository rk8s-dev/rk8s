use crate::chuck::span::SpanIter;
use crate::chuck::{BlockStore, ChunkLayout, ChunkSpan, ChunkWriter, PageTag, SliceDesc, Span};
use crate::meta::{MetaStore, SLICE_ID_KEY};
use crate::vfs::chunk_id_for;
use crate::vfs::inode::Inode;
use dashmap::DashMap;
use dashmap::mapref::one::RefMut;
use sea_orm::sea_query::ExprTrait;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::{Debug, Formatter};
use std::io::{Cursor, Read, Write};
use std::ops::{Deref, DerefMut, Index, IndexMut, Range};
use std::slice::SliceIndex;
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};
use lru::LruCache;

// 4KB
pub const DEFAULT_PAGE_SIZE: u32 = 4 * 1024;

// 256MB
pub const DEFAULT_CACHE_MAX_BYTES: u64 = 256 * 1024 * 1024;

// 256 * 0.8 = 204MB
pub const DEFAULT_LOW_WATERMARK: u64 = 204 * 1024 * 1024;

type DirtyChunkWrite = (u32, Vec<u8>);

type DirtyChunkFlush = (u64, Vec<MarkCleanInfo>, Vec<DirtyChunkWrite>);

/// (ino, chunk_index, page_index)
type AccessKey = (u64, u64, u64);

pub struct CacheBudget {
    current_bytes: Arc<AtomicU64>,
    max_bytes: u64,
    low_watermark: u64,
}

impl Default for CacheBudget {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_MAX_BYTES, DEFAULT_LOW_WATERMARK)
    }
}

impl CacheBudget {
    pub fn new(max_bytes: u64, low_watermark: u64) -> Self {
        Self {
            current_bytes: Arc::new(AtomicU64::new(0)),
            max_bytes,
            low_watermark,
        }
    }

    #[cfg(test)]
    pub fn current_bytes(&self) -> u64 {
        self.current_bytes.load(Ordering::Relaxed)
    }
}

/// Page cache configuration shared across inodes.
#[derive(Clone)]
pub struct PagedCacheConfig {
    pub layout: ChunkLayout,
    pub budget: Arc<CacheBudget>,
    pub page_size: u32,
}

/// Per-inode cache registry.
pub struct Folio {
    caches: DashMap<u64, PagedCache>,
    replacer: Arc<Mutex<LruCache<AccessKey, ()>>>,
    config: Arc<PagedCacheConfig>,
}

impl Folio {
    pub fn new(config: PagedCacheConfig) -> Folio {
        Self {
            caches: DashMap::new(),
            replacer: Arc::new(Mutex::new(LruCache::unbounded())),
            config: Arc::new(config),
        }
    }

    pub fn get_or_create(&self, index: u64) -> RefMut<'_, u64, PagedCache> {
        self.caches
            .entry(index)
            .or_insert_with(|| PagedCache::new(self.config.clone(), self.replacer.clone()))
    }

    pub fn collect_all_ino(&self) -> Vec<u64> {
        self.caches.iter().map(|kv| *kv.key()).collect()
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn evict(&self) {
        let mut current_bytes = self.config.budget.current_bytes.load(Ordering::Relaxed);
        if current_bytes <= self.config.budget.max_bytes {
            return;
        }

        let mut replacer = self.replacer.lock().unwrap();
        let mut attempts = replacer.len();
        while current_bytes > self.config.budget.low_watermark && attempts > 0 {
            let Some((key, _)) = replacer.pop_lru() else {
                break;
            };
            let (ino, chunk_index, page_index) = key;

            if let Some(mut cache) = self.caches.get_mut(&ino)
                && cache.evict_page(chunk_index, page_index)
            {
                current_bytes = self.config.budget.current_bytes.load(Ordering::Relaxed);
                attempts = replacer.len();
                continue;
            }

            replacer.put(key, ());
            attempts -= 1;
        }
    }
}

/// Page cache for a single inode, keyed by chunk index.
pub struct PagedCache {
    pages: DashMap<u64, ChunkPage>,
    replacer: Arc<Mutex<LruCache<AccessKey, ()>>>,
    config: Arc<PagedCacheConfig>,
}

/// Guard holding a mutable chunk page map for a specific chunk index.
pub struct PagedCacheGuard<'a> {
    ino: u64,
    index: u64,
    replacer: Arc<Mutex<LruCache<AccessKey, ()>>>,
    page: RefMut<'a, u64, ChunkPage>,
}

impl<'a> Deref for PagedCacheGuard<'a> {
    type Target = RefMut<'a, u64, ChunkPage>;

    fn deref(&self) -> &Self::Target {
        &self.page
    }
}

impl DerefMut for PagedCacheGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.page
    }
}

/// Access intent for cache probe.
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum AccessKind {
    Write,
    Read,
}

impl<'a> PagedCacheGuard<'a> {
    /// Fill back missing pages for a contiguous range.
    /// `info.len` must be a multiple of `page_size` and aligned to page boundaries.
    pub fn fill_back(&mut self, info: BackfillInfo, buf: &[u8]) {
        let page_size = self.page.config.page_size as usize;
        debug_assert_eq!(buf.len(), info.len as usize);
        debug_assert_eq!(buf.len() % page_size, 0);

        let start = info.offset / self.page.config.page_size;
        let pages = (info.len as usize) / page_size;

        let current_bytes = self.page.config.budget.current_bytes.clone();
        for i in 0..pages {
            let index = (start + i as u32) as u64;
            let page = self.page.ensure_page(index, current_bytes.clone());
            debug_assert!(page.state == PageState::Invalid);

            page.state = PageState::Valid;
            page.padding_to_size(page_size)
                .copy_from_slice(&buf[i * page_size..(i + 1) * page_size]);
        }
    }

    pub fn read_at(&mut self, buf: &mut [u8], offset: u32) -> anyhow::Result<usize> {
        let mut touched = Vec::new();
        let n = self.read_at_with(buf, offset, |index| {
            touched.push(index);
        })?;

        {
            let mut replacer = self.replacer.lock().unwrap();
            for index in touched {
                replacer.put((self.ino, self.index, index), ());
            }
        }
        Ok(n)
    }

    pub fn write_at(&mut self, buf: &[u8], offset: u32) -> anyhow::Result<usize> {
        let mut touched = Vec::new();
        let n = self.write_at_with(buf, offset, |index| {
            touched.push(index);
        })?;

        {
            let mut replacer = self.replacer.lock().unwrap();
            for index in touched {
                replacer.put((self.ino, self.index, index), ());
            }
        }
        Ok(n)
    }
}

impl PagedCache {
    pub fn new(config: Arc<PagedCacheConfig>, replacer: Arc<Mutex<LruCache<AccessKey, ()>>>) -> Self {
        assert_eq!(
            config.layout.chunk_size % config.page_size as u64,
            0,
            "The chunk size must be multiple of page size."
        );

        Self {
            pages: DashMap::default(),
            replacer,
            config,
        }
    }

    pub fn prepare(&self, ino: u64, index: u64) -> PagedCacheGuard<'_> {
        let page = self.ensure_chunk(index);
        PagedCacheGuard { ino, index, page, replacer: self.replacer.clone()}
    }

    pub fn ensure_chunk(&self, index: u64) -> RefMut<'_, u64, ChunkPage> {
        self.pages
            .entry(index)
            .or_insert(ChunkPage::new(self.config.clone()))
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn collect_dirty_chunks(&self) -> Vec<DirtyChunkFlush> {
        let page_size = self.config.page_size;
        let mut results = Vec::new();

        for kv in &self.pages {
            let index = *kv.key();
            let chunk = kv.value();

            let mut info = Vec::new();
            let mut write_op = Vec::new();

            for page in chunk.collect_dirty_pages() {
                info.push(MarkCleanInfo {
                    index: page.index,
                    len: (page.data.len() as u32 / page_size) as u64,
                });

                write_op.push((page.index as u32 * page_size, page.data));
            }

            if !write_op.is_empty() {
                results.push((index, info, write_op));
            }
        }
        results
    }

    #[tracing::instrument(level = "trace", skip(self, index, clean_info))]
    pub fn mark_clean(&mut self, index: u64, clean_info: Vec<MarkCleanInfo>) {
        self.pages
            .get_mut(&index)
            .expect("The chunk which will be marked as clean must exist")
            .mark_clean(clean_info);
    }

    pub fn evict_page(&mut self, chunk_index: u64, page_index: u64) -> bool {
        let mut chunk = match self.pages.get_mut(&chunk_index) {
            Some(chunk) => chunk,
            None => return false,
        };

        chunk.evict_page(page_index)
    }
}

/// Page table for a single chunk.
pub struct ChunkPage {
    pages: BTreeMap<u64, Page>,
    config: Arc<PagedCacheConfig>,
}

impl ChunkPage {
    pub fn new(config: Arc<PagedCacheConfig>) -> Self {
        Self {
            pages: BTreeMap::default(),
            config,
        }
    }

    pub fn ensure_page(&mut self, index: u64, current_bytes: Arc<AtomicU64>) -> &mut Page {
        self.pages.entry(index).or_insert_with(|| Page::new(current_bytes))
    }

    fn evict_page(&mut self, index: u64) -> bool {
        let Some(page) = self.pages.remove(&index) else {
            return false;
        };

        if page.state == PageState::Dirty {
            self.pages.insert(index, page);
            return false;
        }

        let size = page.size();
        if size > 0 {
            page.current_bytes
                .fetch_sub(size as u64, Ordering::Relaxed);
        }
        true
    }

    #[tracing::instrument(level = "trace", skip(self, info))]
    fn mark_clean(&mut self, info: Vec<MarkCleanInfo>) {
        for item in info {
            for index in item.index..(item.index + item.len) {
                let page = self
                    .pages
                    .get_mut(&index)
                    .expect("The page which will be marked as clean must exist");
                if page.state == PageState::Dirty {
                    page.state = PageState::Valid
                }
            }
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn collect_raw_dirty_pages(&self) -> Vec<DirtyPage> {
        self.pages
            .iter()
            .filter(|(_, v)| v.state == PageState::Dirty)
            .map(|(&k, v)| DirtyPage {
                index: k,
                data: v.buf.clone(),
            })
            .collect()
    }

    #[tracing::instrument(level = "trace", skip(self, pages))]
    fn merge_continuous_pages(&self, pages: Vec<DirtyPage>) -> Vec<DirtyPage> {
        let page_size = self.config.page_size;
        let mut results: Vec<DirtyPage> = Vec::new();

        for page in pages {
            if let Some(last) = results.last_mut()
                && last.index + (last.data.len() as u32 / page_size) as u64 == page.index
            {
                last.data.extend(page.data);
                continue;
            }

            results.push(page);
        }
        results
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn collect_dirty_pages(&self) -> Vec<DirtyPage> {
        self.merge_continuous_pages(self.collect_raw_dirty_pages())
    }

    fn split_into_page_span(&self, offset: u32, len: u32) -> SpanIter<PageTag> {
        let chunk_size = self.config.layout.chunk_size;
        let page_size = self.config.page_size;

        let chunk_span = ChunkSpan::new(0, offset, len);
        chunk_span.split_into::<PageTag>(chunk_size, page_size as u64, true)
    }

    /// Probe cache state and return ranges that need backfill.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn probe(&mut self, offset: u32, len: u32, op: AccessKind) -> Vec<BackfillInfo> {
        let page_span = self.split_into_page_span(offset, len);
        let page_size = self.config.page_size;

        let mut results: Vec<BackfillInfo> = Vec::new();

        let current_bytes = self.config.budget.current_bytes.clone();
        for span in page_span {
            let is_full_page = span.offset == 0 && span.len == page_size;
            let page = self.ensure_page(span.index, current_bytes.clone());

            let need_backfill = match op {
                AccessKind::Write => page.state == PageState::Invalid && !is_full_page,
                AccessKind::Read => page.state == PageState::Invalid,
            };

            if need_backfill {
                let offset = span.index as u32 * page_size;
                let len = page_size;

                if let Some(last) = results.last_mut()
                    && last.offset + last.len == offset
                {
                    last.len += len;
                } else {
                    results.push(BackfillInfo { offset, len });
                }
            }
            // Full-page writes don't need old data; state will be updated during write_at.
        }

        results
    }

    pub fn read_at_with(&mut self, mut buf: &mut [u8], offset: u32, mut on_page: impl FnMut(u64)) -> anyhow::Result<usize> {
        let len = buf.len();
        let page_span = self.split_into_page_span(offset, len as u32);

        let mut read_bytes = 0;
        for span in page_span {
            on_page(span.index);
            let slice = self
                .pages
                .get(&span.index)
                .ok_or_else(|| anyhow::anyhow!("Try reading from an invalid page"))?
                .slice(span.offset, span.len)?;

            read_bytes += buf.write(slice)?;
        }
        Ok(read_bytes)

    }

    pub fn write_at_with(&mut self, buf: &[u8], offset: u32, mut on_page: impl FnMut(u64)) -> anyhow::Result<usize> {
        let len = buf.len();
        let page_span = self.split_into_page_span(offset, len as u32);
        let page_size = self.config.page_size;

        let mut cursor = Cursor::new(buf);
        for span in page_span {
            on_page(span.index);
            let page = self.ensure_page(span.index, self.config.budget.current_bytes.clone());

            // Lazily update state when writing
            if page.state == PageState::Invalid && span.offset == 0 && span.len == page_size {
                let target = page.padding_to_size(page_size as usize);
                let _ = cursor.read(target)?;
                page.state = PageState::Dirty;
                continue;
            }

            let slice = page.slice_mut(span.offset, span.len)?;
            let _ = cursor.read(slice)?;
        }
        Ok(cursor.position() as usize)

    }
}

#[derive(Default, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
enum PageState {
    #[default]
    Invalid,
    Dirty,
    Valid,
}

#[derive(Default)]
pub struct Page {
    state: PageState,
    current_bytes: Arc<AtomicU64>,
    buf: Vec<u8>,
}

impl Page {
    fn new(current_bytes: Arc<AtomicU64>) -> Self {
        Self {
            current_bytes,
            ..Default::default()
        }
    }

    fn padding_to_size(&mut self, size: usize) -> &mut [u8] {
        if self.buf.len() < size {
            self.current_bytes.fetch_add((size - self.buf.len()) as u64, Ordering::Relaxed);
            self.buf.resize(size, 0);
        }
        &mut self.buf[..]
    }

    fn size(&self) -> usize {
        self.buf.len()
    }

    pub fn slice(&self, offset: u32, len: u32) -> anyhow::Result<&[u8]> {
        anyhow::ensure!(
            self.state != PageState::Invalid,
            "The page that be read shouldn't be invalid"
        );

        // If the page is valid, we must ensure that the length of buf is long enough.
        let end = (offset + len) as usize;
        Ok(&self.buf[offset as usize..end])
    }

    pub fn slice_mut(&mut self, offset: u32, len: u32) -> anyhow::Result<&mut [u8]> {
        anyhow::ensure!(
            self.state != PageState::Invalid,
            "The page that be written shouldn't be invalid"
        );
        self.state = PageState::Dirty;

        let end = (offset + len) as usize;
        self.padding_to_size(end);
        Ok(&mut self.buf[offset as usize..end])
    }
}

/// Chunk-local backfill range (page-aligned).
#[derive(Copy, Clone)]
pub struct BackfillInfo {
    pub offset: u32,
    pub len: u32,
}

#[derive(Debug)]
pub struct DirtyPage {
    pub index: u64,
    pub data: Vec<u8>,
}

/// Range of pages that can be marked clean after flush.
#[derive(Debug)]
pub struct MarkCleanInfo {
    pub index: u64,
    pub len: u64,
}

#[cfg(test)]
mod tests {
    use super::{AccessKey, AccessKind, CacheBudget, DEFAULT_PAGE_SIZE, Folio, PagedCache, PagedCacheConfig};
    use crate::chuck::ChunkLayout;
    use lru::LruCache;
    use rand::Rng;
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    type Result<T> = anyhow::Result<T>;

    fn backfill_zeroes(
        cache: &'_ PagedCache,
        index: u64,
        offset: u32,
        len: u32,
        op: AccessKind,
    ) -> super::PagedCacheGuard<'_> {
        let mut guard = cache.prepare(0, index);
        if len == 0 {
            return guard;
        }

        let infos = guard.probe(offset, len, op);

        if !infos.is_empty() {
            for info in infos {
                let zero_buf = vec![0u8; info.len as usize];
                guard.fill_back(info, &zero_buf);
            }
        }

        guard
    }

    fn write_with_backfill(
        cache: &PagedCache,
        index: u64,
        offset: u32,
        data: &[u8],
    ) -> Result<usize> {
        let mut guard = backfill_zeroes(cache, index, offset, data.len() as u32, AccessKind::Write);
        guard.write_at(data, offset)
    }

    fn read_with_backfill(
        cache: &PagedCache,
        index: u64,
        offset: u32,
        buf: &mut [u8],
    ) -> Result<usize> {
        let mut guard = backfill_zeroes(cache, index, offset, buf.len() as u32, AccessKind::Read);
        guard.read_at(buf, offset)
    }

    fn build_cache(layout: ChunkLayout, page_size: u32) -> PagedCache {
        let config = PagedCacheConfig {
            layout,
            budget: Arc::new(CacheBudget::default()),
            page_size,
        };
        let replacer = Arc::new(Mutex::new(LruCache::<AccessKey, ()>::unbounded()));
        PagedCache::new(Arc::new(config), replacer)
    }

    fn setup() -> PagedCache {
        build_cache(ChunkLayout::default(), DEFAULT_PAGE_SIZE)
    }

    #[test]
    fn test_basic_write_and_read() -> Result<()> {
        let cache = setup();
        let index = 1;
        let data_to_write = b"Hello Rust";

        let n = write_with_backfill(&cache, index, 0, data_to_write)?;
        assert_eq!(n, data_to_write.len());

        let mut read_buf = vec![0u8; data_to_write.len()];
        let n = read_with_backfill(&cache, index, 0, &mut read_buf)?;

        assert_eq!(n, data_to_write.len());
        assert_eq!(read_buf, data_to_write);
        Ok(())
    }

    #[test]
    fn test_offset_handling() -> Result<()> {
        let cache = setup();
        let index = 1;

        write_with_backfill(&cache, index, 10, b"World")?;

        write_with_backfill(&cache, index, 0, b"Hello")?;

        let mut buf_world = vec![0u8; 5];
        read_with_backfill(&cache, index, 10, &mut buf_world)?;
        assert_eq!(buf_world, b"World");

        let mut buf_hello = vec![0u8; 5];
        read_with_backfill(&cache, index, 0, &mut buf_hello)?;
        assert_eq!(buf_hello, b"Hello");

        let mut buf_gap = vec![0u8; 5];
        read_with_backfill(&cache, index, 5, &mut buf_gap)?;
        assert_eq!(buf_gap, [0, 0, 0, 0, 0]);

        Ok(())
    }

    #[test]
    fn test_index_isolation() -> Result<()> {
        let cache = setup();

        write_with_backfill(&cache, 100, 0, &[0xAA; 4])?;

        write_with_backfill(&cache, 200, 0, &[0xBB; 4])?;

        let mut buf = vec![0u8; 4];
        read_with_backfill(&cache, 100, 0, &mut buf)?;
        assert_eq!(buf, &[0xAA; 4]);

        read_with_backfill(&cache, 200, 0, &mut buf)?;
        assert_eq!(buf, &[0xBB; 4]);

        Ok(())
    }

    #[test]
    fn test_partial_overwrite() -> Result<()> {
        let cache = setup();
        let index = 5;

        write_with_backfill(&cache, index, 0, &[1u8; 10])?;

        write_with_backfill(&cache, index, 4, &[2u8; 2])?;

        let mut buf = vec![0u8; 10];
        read_with_backfill(&cache, index, 0, &mut buf)?;

        let expected = [1, 1, 1, 1, 2, 2, 1, 1, 1, 1];
        assert_eq!(buf, expected);

        Ok(())
    }

    #[test]
    fn test_read_requires_backfill() -> Result<()> {
        let cache = build_cache(
            ChunkLayout {
                block_size: 1024,
                chunk_size: 256,
            },
            64,
        );

        let mut guard = cache.prepare(0, 0);
        let mut buf = vec![0u8; 8];
        assert!(guard.read_at(&mut buf, 0).is_err());

        let infos = guard.probe(0, buf.len() as u32, AccessKind::Read);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].offset, 0);
        assert_eq!(infos[0].len, 64);

        let mut page = vec![0u8; 64];
        for (i, b) in page.iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(1);
        }
        guard.fill_back(infos[0], &page);

        guard.read_at(&mut buf, 0)?;
        assert_eq!(&buf[..], &page[..8]);
        Ok(())
    }

    #[test]
    fn test_probe_backfill_ranges_across_pages() {
        let cache = build_cache(
            ChunkLayout {
                block_size: 1024,
                chunk_size: 256,
            },
            64,
        );

        let mut guard = cache.prepare(0, 0);
        let infos = guard.probe(32, 100, AccessKind::Read);

        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].offset, 0);
        assert_eq!(infos[0].len, 64 * 3);
    }

    #[test]
    fn test_partial_write_backfill_preserves_page() -> Result<()> {
        let cache = build_cache(
            ChunkLayout {
                block_size: 1024,
                chunk_size: 256,
            },
            64,
        );

        let mut guard = cache.prepare(0, 0);
        let infos = guard.probe(4, 4, AccessKind::Write);
        assert_eq!(infos.len(), 1);

        let page = vec![0x11u8; 64];
        guard.fill_back(infos[0], &page);

        guard.write_at(&[0x22u8; 4], 4)?;

        let mut buf = vec![0u8; 8];
        guard.read_at(&mut buf, 0)?;
        assert_eq!(&buf[..4], &[0x11u8; 4]);
        assert_eq!(&buf[4..], &[0x22u8; 4]);
        Ok(())
    }

    #[test]
    fn test_full_page_span_write_without_backfill() -> Result<()> {
        let cache = build_cache(
            ChunkLayout {
                block_size: 1024,
                chunk_size: 256,
            },
            64,
        );

        // offset=99, len=93 -> spans a partial page then a full page.
        let offset = 99u32;
        let data = vec![0xABu8; 93];

        let n = write_with_backfill(&cache, 0, offset, &data)?;
        assert_eq!(n, data.len());

        let mut out = vec![0u8; data.len()];
        let n = read_with_backfill(&cache, 0, offset, &mut out)?;
        assert_eq!(n, data.len());
        assert_eq!(out, data);
        Ok(())
    }

    #[test]
    fn fuzz_test_cache_consistency() {
        let mut rng = rand::rng();

        let chunk_size: usize = 256;
        let page_size = 64;

        let iterations = 10_000;

        let cache = build_cache(
            ChunkLayout {
                block_size: 64,
                chunk_size: 256,
            },
            page_size,
        );

        let mut model: HashMap<u64, Vec<u8>> = HashMap::new();

        for i in 0..iterations {
            let index: u64 = rng.random_range(0..20);

            let len: usize = rng.random_range(0..=chunk_size);

            let max_offset = chunk_size - len;
            let offset: u32 = rng.random_range(0..=max_offset) as u32;

            if rng.random_bool(0.5) {
                let mut write_buf = vec![0u8; len];
                rng.fill(&mut write_buf[..]);

                write_with_backfill(&cache, index, offset, &write_buf)
                    .expect("Write valid data failed");

                let model_data = model.entry(index).or_insert_with(|| vec![0u8; chunk_size]);
                let end = offset as usize + len;
                model_data[offset as usize..end].copy_from_slice(&write_buf);
            } else {
                let mut read_buf = vec![0u8; len];

                read_with_backfill(&cache, index, offset, &mut read_buf)
                    .expect("Read valid data failed");

                let expected = match model.get(&index) {
                    Some(data) => {
                        let end = offset as usize + len;
                        &data[offset as usize..end]
                    }
                    None => &[],
                };

                if expected.is_empty() {
                    assert_eq!(
                        read_buf,
                        vec![0u8; len],
                        "Iteration {}: Read uninitialized data at index {}, expect zeros",
                        i,
                        index
                    );
                } else {
                    assert_eq!(
                        read_buf, expected,
                        "Iteration {}: Data mismatch at index {}, offset {}, len {}",
                        i, index, offset, len
                    );
                }
            }
        }
    }

    #[test]
    fn test_evict_removes_clean_pages() {
        let page_size = 16u32;
        let budget = Arc::new(CacheBudget::new(page_size as u64, 0));
        let config = PagedCacheConfig {
            layout: ChunkLayout {
                block_size: 64,
                chunk_size: 64,
            },
            budget: budget.clone(),
            page_size,
        };
        let folio = Folio::new(config);

        let ino = 1u64;
        {
            let cache = folio.get_or_create(ino);
            let mut guard = cache.prepare(ino, 0);

            let infos = guard.probe(0, page_size * 2, AccessKind::Read);
            assert_eq!(infos.len(), 1);
            let buf = vec![0xABu8; infos[0].len as usize];
            guard.fill_back(infos[0], &buf);

            let mut tmp = vec![0u8; 1];
            guard.read_at(&mut tmp, 0).unwrap();
            guard.read_at(&mut tmp, page_size).unwrap();
        }

        assert_eq!(
            budget.current_bytes.load(Ordering::Relaxed),
            (page_size * 2) as u64
        );
        folio.evict();
        assert_eq!(budget.current_bytes.load(Ordering::Relaxed), 0);

        let cache = folio.get_or_create(ino);
        let chunk = cache.pages.get(&0).expect("chunk should exist");
        assert!(chunk.pages.is_empty());
    }

    #[test]
    fn test_evict_skips_dirty_pages() -> Result<()> {
        let page_size = 16u32;
        let budget = Arc::new(CacheBudget::new(page_size as u64, 0));
        let config = PagedCacheConfig {
            layout: ChunkLayout {
                block_size: 64,
                chunk_size: 64,
            },
            budget: budget.clone(),
            page_size,
        };
        let folio = Folio::new(config);

        let ino = 2u64;
        {
            let cache = folio.get_or_create(ino);
            let mut guard = cache.prepare(ino, 0);
            let data = vec![0xCDu8; page_size as usize];
            guard.write_at(&data, 0)?;
        }

        assert_eq!(budget.current_bytes.load(Ordering::Relaxed), page_size as u64);
        folio.evict();
        assert_eq!(budget.current_bytes.load(Ordering::Relaxed), page_size as u64);

        let cache = folio.get_or_create(ino);
        let chunk = cache.pages.get(&0).expect("chunk should exist");
        assert_eq!(chunk.pages.len(), 1);
        Ok(())
    }
}
