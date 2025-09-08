//! 存储后端抽象：块级读写接口 + 内存实现（异步）。

use super::chunk::ChunkLayout;
use crate::{
    cadapter::client::{ObjectBackend, ObjectClient},
    chuck::cache::CacheItem,
};
use async_trait::async_trait;
use hex::encode;
use libc::{KEYCTL_CAPS0_CAPABILITIES, SYS_remap_file_pages};
use moka::{Entry, ops::compute::Op};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs, io::SeekFrom, path::PathBuf};
use tokio::io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// 抽象块存储接口（后续可由 cadapter/S3 等实现）。
#[async_trait]
// ensure offset_in_block + data.len() <= block_size
pub trait BlockStore {
    async fn write_block_range(
        &mut self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        data: &[u8],
        layout: ChunkLayout,
    );

    async fn read_block_range(
        &self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        len: usize,
        layout: ChunkLayout,
    ) -> Vec<u8>;
}

type BlockKey = (i64 /*chunk_id*/, u32 /*block_index*/);

/// 简单内存实现：用于本地开发/测试。
#[derive(Default)]
pub struct InMemoryBlockStore {
    map: HashMap<BlockKey, Vec<u8>>, // 每个块固定大小
}

impl InMemoryBlockStore {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn ensure_block(&mut self, key: BlockKey, block_size: usize) -> &mut Vec<u8> {
        let entry = self.map.entry(key).or_insert_with(|| vec![0u8; block_size]);
        if entry.len() < block_size {
            entry.resize(block_size, 0);
        }
        entry
    }
}

#[async_trait]
impl BlockStore for InMemoryBlockStore {
    async fn write_block_range(
        &mut self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        data: &[u8],
        layout: ChunkLayout,
    ) {
        let block_size = layout.block_size as usize;
        let buf = self.ensure_block((chunk_id, block_index), block_size);
        let start = offset_in_block as usize;
        let end = start + data.len();
        debug_assert!(end <= block_size, "write exceeds block boundary");
        buf[start..end].copy_from_slice(data);
    }

    async fn read_block_range(
        &self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        len: usize,
        layout: ChunkLayout,
    ) -> Vec<u8> {
        let start = offset_in_block as usize;
        let end = start + len;
        if let Some(buf) = self.map.get(&(chunk_id, block_index)) {
            let mut out = vec![0u8; len];
            let copy_end = end.min(buf.len());
            if copy_end > start {
                out[..(copy_end - start)].copy_from_slice(&buf[start..copy_end]);
            }
            out
        } else {
            // 未写入的洞返回 0
            let _ = layout; // 抑制未使用
            vec![0u8; len]
        }
    }
}

/// 通过 cadapter::client 后端实现的 BlockStore（键空间：chunks/{chunk_id}/{block_index}）。
pub struct ObjectBlockStore<B: ObjectBackend> {
    client: ObjectClient<B>,
    block_cache: moka::future::Cache<String, CacheItem>,
    cache_dir: PathBuf,
}

impl<B: ObjectBackend> ObjectBlockStore<B> {
    pub fn new(client: ObjectClient<B>) -> Self {
        let cache_dir = dirs::cache_dir().unwrap().join("slayerfs");

        let _ = fs::create_dir_all(cache_dir.clone());

        Self {
            client,
            block_cache: moka::future::Cache::new(10_000),
            cache_dir,
        }
    }

    fn key_for(chunk_id: i64, block_index: u32) -> String {
        format!("chunks/{chunk_id}/{block_index}")
    }

    fn file_name_from_key(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        encode(hasher.finalize())
    }
    async fn handle_existing_cache_entry(
        &self,
        cache_entry: &Entry<String, CacheItem>, // 假设 CacheEntry 类型
        key: &str,
        start: usize,
        end: usize,
        buf: &mut [u8],
    ) -> Op<CacheItem> {
        let local_etag = cache_entry.value().etag.clone();
        let remote_etag = self.client.get_etag(key).await.unwrap();
        let mut file_path = cache_entry.value().file_path.clone();

        if local_etag == remote_etag {
            self.read_from_valid_cache(file_path.clone(), start, buf, key)
                .await
        } else {
            file_path = self.cache_dir.join(Self::file_name_from_key(key));
            self.fetch_from_object_store(key, start, end, buf, file_path.clone())
                .await
        };

        Op::Put(CacheItem {
            file_path,
            etag: remote_etag,
        })
    }

    async fn handle_new_cache_entry(
        &self,
        key: &str,
        start: usize,
        end: usize,
        buf: &mut [u8],
        layout: ChunkLayout,
    ) -> Op<CacheItem> {
        let file_path = self.cache_dir.join(Self::file_name_from_key(key));
        let object = self
            .fetch_object_or_zero(key, layout.block_size as usize)
            .await;
        let etag = self.client.get_etag(key).await.unwrap();

        buf.copy_from_slice(&object[start..end]);
        self.write_to_cache_async(file_path.clone(), object).await;

        Op::Put(CacheItem { file_path, etag })
    }

    async fn read_from_valid_cache(
        &self,
        file_path: PathBuf,
        start: usize,
        buf: &mut [u8],
        key: &str,
    ) {
        let mut file = match tokio::fs::File::open(file_path.clone()).await {
            Ok(file) => file,
            Err(e) => {
                panic!("failed to open cache file: {e}");
            }
        };

        if let Err(e) = file.seek(SeekFrom::Start(start as u64)).await {
            panic!("failed to seek in cache file: {e}");
        }

        match file.read_exact(buf).await {
            Ok(_) => {
                // println!("hits");
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                println!("cache file corrupted, falling back to object store");
                self.handle_corrupted_cache(key, start, buf.len(), buf, file_path)
                    .await;
            }
            Err(e) => {
                panic!("failed to read from cache file: {e}");
            }
        }
    }

    async fn handle_corrupted_cache(
        &self,
        key: &str,
        start: usize,
        len: usize,
        buf: &mut [u8],
        file_path: PathBuf,
    ) {
        let object = self.fetch_object_or_zero(key, len).await;

        if start + len <= object.len() {
            buf.copy_from_slice(&object[start..start + len]);
        } else {
            // Handle case where requested range exceeds object size
            buf.fill(0);
        }

        self.write_to_cache_async(file_path, object).await;
    }

    async fn fetch_from_object_store(
        &self,
        key: &str,
        start: usize,
        end: usize,
        buf: &mut [u8],
        file_path: PathBuf,
    ) {
        let object = self.fetch_object_or_zero(key, end - start).await;
        buf.copy_from_slice(&object[start..end]);
        self.write_to_cache_async(file_path, object).await;
    }

    async fn fetch_object_or_zero(&self, key: &str, default_size: usize) -> Vec<u8> {
        match self.client.get_object(key).await.unwrap() {
            Some(obj) => obj,
            None => vec![0u8; default_size],
        }
    }

    async fn write_to_cache_async(&self, file_path: PathBuf, data: Vec<u8>) {
        tokio::spawn(async move {
            if let Ok(mut file) = tokio::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&file_path)
                .await
            {
                if let Err(e) = file.seek(SeekFrom::Start(0)).await {
                    eprintln!("Failed to seek in file {}: {}", file_path.display(), e);
                    return;
                }
                if let Err(e) = file.write_all(&data).await {
                    eprintln!("Failed to write to file {}: {}", file_path.display(), e);
                }
            }
        });
    }
}

#[async_trait]
impl<B: ObjectBackend + Send + Sync> BlockStore for ObjectBlockStore<B> {
    async fn write_block_range(
        &mut self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        data: &[u8],
        layout: ChunkLayout,
    ) {
        // 读取已有对象（若存在），在内存拼接后整体写回；MVP 简化。
        let key = Self::key_for(chunk_id, block_index);
        let bs = layout.block_size as usize;
        // 失败直接 panic，与原同步实现行为一致；后续可改为返回 Result。
        let existing = self
            .client
            .get_object(&key)
            .await
            .expect("object store get failed");
        let mut buf = existing.unwrap_or_else(|| vec![0u8; bs]);
        if buf.len() < bs {
            buf.resize(bs, 0);
        }
        let start = offset_in_block as usize;
        let end = start + data.len();
        buf[start..end].copy_from_slice(data);
        self.client
            .put_object(&key, &buf)
            .await
            .expect("object store put failed");
    }

    async fn read_block_range(
        &self,
        chunk_id: i64,
        block_index: u32,
        offset_in_block: u32,
        len: usize,
        layout: ChunkLayout,
    ) -> Vec<u8> {
        let key = Self::key_for(chunk_id, block_index);
        let start = offset_in_block as usize;
        let end = start + len;
        let mut buf = vec![0u8; len];

        let entry = self.block_cache.entry(key.clone());

        entry
            .and_compute_with(async |item| match item {
                Some(cache_entry) => {
                    self.handle_existing_cache_entry(&cache_entry, &key, start, end, &mut buf)
                        .await
                }
                None => {
                    self.handle_new_cache_entry(&key, start, end, &mut buf, layout)
                        .await
                }
            })
            .await;

        buf
    }
}

/// 便捷别名：基于真实 S3Backend 的 BlockStore
#[allow(dead_code)]
pub type S3BlockStore = ObjectBlockStore<crate::cadapter::s3::S3Backend>;
/// 便捷别名：基于 RustfsLikeBackend 的 BlockStore
#[allow(dead_code)]
pub type RustfsBlockStore = ObjectBlockStore<crate::cadapter::rustfs::RustfsLikeBackend>;
/// 便捷别名：基于 LocalFsBackend 的 BlockStore（mock 本地目录）
#[allow(dead_code)]
pub type LocalFsBlockStore = ObjectBlockStore<crate::cadapter::localfs::LocalFsBackend>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cadapter::client::ObjectClient;
    use crate::cadapter::localfs::LocalFsBackend;
    use crate::chuck::chunk::ChunkLayout;

    #[tokio::test]
    async fn test_localfs_block_store_put_get() {
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let mut store = ObjectBlockStore::new(client);
        let layout = ChunkLayout::default();

        let data = vec![7u8; layout.block_size as usize / 2];
        store
            .write_block_range(42, 3, layout.block_size / 4, &data, layout)
            .await;

        let out = store
            .read_block_range(42, 3, layout.block_size / 4, data.len(), layout)
            .await;
        assert_eq!(out, data);
    }

    #[tokio::test]
    async fn test_cache_effectiveness() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let mut store = ObjectBlockStore::new(client);
        let layout = ChunkLayout::default();
        let data = vec![7u8; layout.block_size as usize / 2];
        store
            .write_block_range(42, 3, layout.block_size / 4, &data, layout)
            .await;
        // 第一次读取 - 应该缓存未命中
        let data1 = store
            .read_block_range(42, 3, layout.block_size / 4, data.len(), layout)
            .await;

        // 第二次读取相同数据 - 应该缓存命中
        let data2 = store
            .read_block_range(42, 3, layout.block_size / 4, data.len(), layout)
            .await;
        assert_eq!(data1, data2);

        Ok(())
    }
}
