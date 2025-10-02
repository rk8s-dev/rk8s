//! 存储后端抽象：块级读写接口 + 内存实现（异步）。

use super::chunk::ChunkLayout;
use crate::{
    cadapter::client::{ObjectBackend, ObjectClient},
    chuck::cache::{ChunksCache, ChunksCacheConfig},
};
use anyhow;
use async_trait::async_trait;
use futures::executor::block_on;
use hex::encode;
use libc::{KEYCTL_CAPS0_CAPABILITIES, SYS_remap_file_pages, VM_VFS_CACHE_PRESSURE};
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

    #[allow(dead_code)]
    async fn delete_block_range(
        &mut self,
        chunk_id: i64,
        block_index: u32,
        len: usize,
    ) -> anyhow::Result<()>;
}

#[allow(dead_code)]
type BlockKey = (i64 /*chunk_id*/, u32 /*block_index*/);

/// 简单内存实现：用于本地开发/测试。
#[derive(Default)]
#[allow(dead_code)]
pub struct InMemoryBlockStore {
    map: HashMap<BlockKey, Vec<u8>>, // 每个块固定大小
}

#[allow(dead_code)]
impl InMemoryBlockStore {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    #[allow(dead_code)]
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

    // Delete the block range [block_index,blcok_inde + len)
    async fn delete_block_range(
        &mut self,
        chunk_id: i64,
        block_index: u32,
        len: usize,
    ) -> anyhow::Result<()> {
        let start = block_index;
        let end = start + len as u32;
        for i in start..end {
            self.map.remove(&(chunk_id, i));
        }
        Ok(())
    }
}

/// 通过 cadapter::client 后端实现的 BlockStore（键空间：chunks/{chunk_id}/{block_index}）。
pub struct ObjectBlockStore<B: ObjectBackend> {
    client: ObjectClient<B>,
    block_cache: ChunksCache,
}

impl<B: ObjectBackend> ObjectBlockStore<B> {
    pub fn new(client: ObjectClient<B>) -> Self {
        let cache_dir = dirs::cache_dir().unwrap().join("slayerfs");

        let _ = fs::create_dir_all(cache_dir.clone());

        let block_cache = block_on(ChunksCache::new_with_config(ChunksCacheConfig::default()))
            .map_err(|e| anyhow::anyhow!("Failed to create cache: {}", e))
            .unwrap();
        Self {
            client,
            block_cache,
        }
    }
    /// Creates a new ObjectBlockStore with custom cache configuration
    #[allow(dead_code)]
    pub fn new_with_config(
        client: ObjectClient<B>,
        config: ChunksCacheConfig,
    ) -> anyhow::Result<Self> {
        let cache_dir = dirs::cache_dir().unwrap().join("slayerfs");
        let _ = fs::create_dir_all(cache_dir.clone());

        let block_cache = block_on(ChunksCache::new_with_config(config))
            .map_err(|e| anyhow::anyhow!("Failed to create cache: {}", e))?;
        Ok(Self {
            client,
            block_cache,
        })
    }

    fn key_for(chunk_id: i64, block_index: u32) -> String {
        format!("chunks/{chunk_id}/{block_index}")
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

        let etag = self
            .client
            .get_etag(&key)
            .await
            .unwrap_or_else(|_| "default_etag".to_string());
        let cache_key = format!("{}{}", key, etag);
        let _ = self.block_cache.remove(&cache_key).await;
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
        let _ = layout;

        let etag = self
            .client
            .get_etag(&key)
            .await
            .unwrap_or_else(|_| "default_etag".to_string());
        let cache_key = format!("{}{}", key, etag);
        match self.block_cache.get(&cache_key).await {
            Some(block) => {
                buf.copy_from_slice(&block[start..end]);
            }
            None => {
                let block = self
                    .client
                    .get_object(&key)
                    .await
                    .unwrap()
                    .unwrap_or_else(|| vec![0u8; len]);
                buf.copy_from_slice(&block[start..end]);
                self.block_cache.insert(&cache_key, &block).await.unwrap();
            }
        }

        buf
    }

    async fn delete_block_range(
        &mut self,
        chunk_id: i64,
        block_index: u32,
        len: usize,
    ) -> anyhow::Result<()> {
        let start = block_index;
        let end = start + len as u32;

        for i in start..end {
            let key = Self::key_for(chunk_id, i);
            self.client.delete_object(&key).await.unwrap();
        }

        Ok(())
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
