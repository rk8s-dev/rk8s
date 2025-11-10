//! Storage backends: asynchronous block-level IO traits and in-memory implementations.

use crate::{
    cadapter::client::{ObjectBackend, ObjectClient},
    chuck::cache::{ChunksCache, ChunksCacheConfig},
};
use anyhow::{self, Context};
use async_trait::async_trait;
use futures::executor::block_on;
use hex::encode;
use libc::{KEYCTL_CAPS0_CAPABILITIES, SYS_remap_file_pages, VM_VFS_CACHE_PRESSURE};
use moka::{Entry, ops::compute::Op};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs, io::SeekFrom, path::PathBuf};
use tokio::{
    io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::RwLock,
};
use tracing::info;

/// Abstract block store interface (cadapter/S3/etc. can implement this).
#[async_trait]
// ensure offset_in_block + data.len() <= block_size
pub trait BlockStore {
    async fn write_range(&self, key: BlockKey, offset: u32, data: &[u8]) -> anyhow::Result<u64>;

    async fn read_range(&self, key: BlockKey, offset: u32, len: usize) -> anyhow::Result<Vec<u8>>;

    #[allow(dead_code)]
    async fn delete_range(&self, key: BlockKey, len: usize) -> anyhow::Result<()>;
}

pub type BlockKey = (i64 /*chunk_id*/, u32 /*block_index*/);

/// Simple in-memory implementation for local development/testing.
#[derive(Default)]
#[allow(dead_code)]
pub struct InMemoryBlockStore {
    map: RwLock<HashMap<BlockKey, Vec<u8>>>,
}

#[allow(dead_code)]
impl InMemoryBlockStore {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            map: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl BlockStore for InMemoryBlockStore {
    async fn write_range(&self, key: BlockKey, offset: u32, data: &[u8]) -> anyhow::Result<u64> {
        let mut guard = self.map.write().await;
        let entry = guard.entry(key).or_insert_with(Vec::new);
        let start = offset as usize;
        let end = start + data.len();
        if entry.len() < end {
            entry.resize(end, 0);
        }
        entry[start..end].copy_from_slice(data);
        Ok(data.len() as u64)
    }

    async fn read_range(&self, key: BlockKey, offset: u32, len: usize) -> anyhow::Result<Vec<u8>> {
        let guard = self.map.read().await;
        let mut out = vec![0u8; len];
        if let Some(buf) = guard.get(&key) {
            let start = offset as usize;
            let end = start + len;
            let copy_end = end.min(buf.len());
            if copy_end > start {
                out[..(copy_end - start)].copy_from_slice(&buf[start..copy_end]);
            }
        }
        Ok(out)
    }

    async fn delete_range(&self, key: BlockKey, len: usize) -> anyhow::Result<()> {
        let (chunk_id, block_index) = key;
        let mut guard = self.map.write().await;
        let start = block_index;
        let end = start + len as u32;
        for i in start..end {
            guard.remove(&(chunk_id, i));
        }
        Ok(())
    }
}

/// BlockStore backed by cadapter::client (key space `chunks/{chunk_id}/{block_index}`).
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

    fn key_for(key: BlockKey) -> String {
        let (chunk_id, block_index) = key;
        format!("chunks/{chunk_id}/{block_index}")
    }
}

#[async_trait]
impl<B: ObjectBackend + Send + Sync> BlockStore for ObjectBlockStore<B> {
    async fn write_range(&self, key: BlockKey, offset: u32, data: &[u8]) -> anyhow::Result<u64> {
        let key_str = Self::key_for(key);
        let existing = self.client.get_object(&key_str).await;
        let mut buf = match existing {
            Ok(Some(data)) => data,
            Ok(None) => Vec::new(),
            Err(e) => {
                let error_str = format!("{:?}", e);
                if error_str.contains("NoSuchKey") || error_str.contains("NotFound") {
                    Vec::new()
                } else {
                    return Err(anyhow::anyhow!("object store get failed: {:?}", e));
                }
            }
        };
        let start = offset as usize;
        let end = start + data.len();
        if buf.len() < end {
            buf.resize(end, 0);
        }
        buf[start..end].copy_from_slice(data);
        self.client
            .put_object(&key_str, &buf)
            .await
            .map_err(|e| anyhow::anyhow!("object store put failed: {key_str}, {e:?}"))?;

        let etag = self
            .client
            .get_etag(&key_str)
            .await
            .unwrap_or_else(|_| "default_etag".to_string());
        let cache_key = format!("{}{}", key_str, etag);
        let _ = self.block_cache.remove(&cache_key).await;
        Ok(data.len() as u64)
    }

    async fn read_range(&self, key: BlockKey, offset: u32, len: usize) -> anyhow::Result<Vec<u8>> {
        let key_str = Self::key_for(key);
        let start = offset as usize;
        let end = start + len;
        let mut buf = vec![0u8; len];

        let etag = self
            .client
            .get_etag(&key_str)
            .await
            .unwrap_or_else(|_| "default_etag".to_string());
        let cache_key = format!("{}{}", key_str, etag);
        if let Some(block) = self.block_cache.get(&cache_key).await {
            let copy_end = end.min(block.len());
            if copy_end > start {
                buf[..(copy_end - start)].copy_from_slice(&block[start..copy_end]);
            }
            info!("Read block range from cache");
            return Ok(buf);
        }

        let block = match self.client.get_object(&key_str).await {
            Ok(Some(data)) => data,
            Ok(None) => vec![0u8; end],
            Err(e) => {
                let error_str = format!("{:?}", e);
                if error_str.contains("NoSuchKey") || error_str.contains("NotFound") {
                    vec![0u8; end]
                } else {
                    return Err(anyhow::anyhow!("object store get failed: {:?}", e));
                }
            }
        };
        let copy_end = end.min(block.len());
        if copy_end > start {
            buf[..(copy_end - start)].copy_from_slice(&block[start..copy_end]);
        }
        let _ = self.block_cache.insert(&cache_key, &block).await;
        Ok(buf)
    }

    async fn delete_range(&self, key: BlockKey, len: usize) -> anyhow::Result<()> {
        let (chunk_id, block_index) = key;
        let start = block_index;
        let end = start + len as u32;
        for i in start..end {
            let key_str = Self::key_for((chunk_id, i));
            self.client
                .delete_object(&key_str)
                .await
                .map_err(|e| anyhow::anyhow!("object store delete failed: {key_str}, {e:?}"))?;
        }
        Ok(())
    }
}

/// Convenience alias: BlockStore backed by the real S3 backend.
#[allow(dead_code)]
pub type S3BlockStore = ObjectBlockStore<crate::cadapter::s3::S3Backend>;
/// Convenience alias: BlockStore backed by the Rustfs-like backend.
#[allow(dead_code)]
pub type RustfsBlockStore = ObjectBlockStore<crate::cadapter::rustfs::RustfsLikeBackend>;
/// Convenience alias: BlockStore backed by the LocalFs mock backend.
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
        let store = ObjectBlockStore::new(client);
        let layout = ChunkLayout::default();

        let data = vec![7u8; layout.block_size as usize / 2];
        store
            .write_range((42, 3), layout.block_size / 4, &data)
            .await
            .unwrap();

        let out = store
            .read_range((42, 3), layout.block_size / 4, data.len())
            .await
            .unwrap();
        assert_eq!(out, data);
    }

    #[tokio::test]
    async fn test_cache_effectiveness() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let store = ObjectBlockStore::new(client);
        let layout = ChunkLayout::default();
        let data = vec![7u8; layout.block_size as usize / 2];
        store
            .write_range((42, 3), layout.block_size / 4, &data)
            .await
            .unwrap();
        // First read should miss the cache.
        let data1 = store
            .read_range((42, 3), layout.block_size / 4, data.len())
            .await
            .unwrap();

        // Second read of the same data should hit the cache.
        let data2 = store
            .read_range((42, 3), layout.block_size / 4, data.len())
            .await
            .unwrap();
        assert_eq!(data1, data2);

        Ok(())
    }
}
