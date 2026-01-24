//! Storage backends: asynchronous block-level IO traits and in-memory implementations.

use crate::utils::zero::make_zero_bytes;
use crate::{
    cadapter::client::{ObjectBackend, ObjectClient},
    chuck::cache::{ChunksCache, ChunksCacheConfig},
};
use anyhow::{self, Context};
use async_trait::async_trait;
use bytes::Bytes;
use futures::executor::block_on;
use hex::encode;
use moka::{Entry, ops::compute::Op};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs, io::SeekFrom, path::PathBuf, sync::LazyLock};
use tokio::{
    io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::RwLock,
};
use tracing::info;

/// Abstract block store interface (cadapter/S3/etc. can implement this).
#[async_trait]
// ensure offset_in_block + data.len() <= block_size
pub trait BlockStore {
    /// Write a new block from a set of byte segments without concatenating them.
    /// The result of it should be equal to concatenate these bytes and call [`write_range`].
    async fn write_vectored(
        &self,
        key: BlockKey,
        offset: u32,
        chunks: Vec<Bytes>,
    ) -> anyhow::Result<u64> {
        let data = chunks
            .into_iter()
            .flat_map(|e| e.to_vec())
            .collect::<Vec<_>>();
        self.write_range(key, offset, &data).await
    }

    async fn write_range(&self, key: BlockKey, offset: u32, data: &[u8]) -> anyhow::Result<u64>;

    /// Write a new block without reading any existing data.
    ///
    /// This exists to support COW-style writes where every write targets a fresh object/key.
    /// In that model, read-modify-write is wasted IO because there is no old data to preserve.
    /// Callers must ensure the target key is fresh; using this on an existing object would
    /// drop any previous content outside the written range.
    async fn write_fresh_vectored(
        &self,
        key: BlockKey,
        offset: u32,
        chunks: Vec<Bytes>,
    ) -> anyhow::Result<u64> {
        self.write_vectored(key, offset, chunks).await
    }

    /// Write a new block without reading any existing data. See write_fresh_vectored for details.
    async fn write_fresh_range(
        &self,
        key: BlockKey,
        offset: u32,
        data: &[u8],
    ) -> anyhow::Result<u64> {
        self.write_range(key, offset, data).await
    }

    async fn read_range(&self, key: BlockKey, offset: u32, buf: &mut [u8]) -> anyhow::Result<()>;

    #[allow(dead_code)]
    async fn delete_range(&self, key: BlockKey, len: usize) -> anyhow::Result<()>;
}

pub type BlockKey = (u64 /*slice_id*/, u32 /*block_index*/);

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

    async fn read_range(&self, key: BlockKey, offset: u32, buf: &mut [u8]) -> anyhow::Result<()> {
        buf.fill(0);
        let guard = self.map.read().await;
        if let Some(src) = guard.get(&key) {
            let start = offset as usize;
            let end = start + buf.len();
            let copy_end = end.min(src.len());
            if copy_end > start {
                let len = copy_end - start;
                buf[..len].copy_from_slice(&src[start..copy_end]);
            }
        }
        Ok(())
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
    async fn write_vectored(
        &self,
        key: BlockKey,
        offset: u32,
        chunks: Vec<Bytes>,
    ) -> anyhow::Result<u64> {
        let key_str = Self::key_for(key);
        let total_len = chunks.iter().map(|c| c.len()).sum::<usize>();
        if total_len == 0 {
            return Ok(0);
        }

        let existing = self
            .client
            .get_object(&key_str)
            .await
            .map_err(|e| anyhow::anyhow!("object store get failed: {:?}", e))?;

        let offset_usize = offset as usize;
        let mut parts: Vec<Bytes> = Vec::new();

        if let Some(existing_vec) = existing {
            let existing_bytes = Bytes::from(existing_vec);
            let existing_len = existing_bytes.len();

            let prefix_take = offset_usize.min(existing_len);
            if prefix_take > 0 {
                parts.push(existing_bytes.slice(0..prefix_take));
            }
            if existing_len < offset_usize {
                parts.extend(make_zero_bytes(offset_usize - existing_len));
            }

            parts.extend(chunks.iter().cloned());

            let end = offset_usize + total_len;
            if existing_len > end {
                parts.push(existing_bytes.slice(end..existing_len));
            }
        } else {
            if offset_usize > 0 {
                parts.extend(make_zero_bytes(offset_usize));
            }
            parts.extend(chunks.iter().cloned());
        }

        self.client
            .put_object_vectored(&key_str, parts)
            .await
            .map_err(|e| anyhow::anyhow!("object store put failed: {key_str}, {e:?}"))?;

        let etag = self
            .client
            .get_etag(&key_str)
            .await
            .unwrap_or_else(|_| "default_etag".to_string());
        let cache_key = format!("{}{}", key_str, etag);
        let _ = self.block_cache.remove(&cache_key).await;
        Ok(total_len as u64)
    }

    async fn write_fresh_vectored(
        &self,
        key: BlockKey,
        offset: u32,
        chunks: Vec<Bytes>,
    ) -> anyhow::Result<u64> {
        let key_str = Self::key_for(key);
        let total_len = chunks.iter().map(|c| c.len()).sum::<usize>();
        if total_len == 0 {
            return Ok(0);
        }

        let offset_usize = offset as usize;
        let mut parts: Vec<Bytes> = Vec::new();
        if offset_usize > 0 {
            parts.extend(make_zero_bytes(offset_usize));
        }
        parts.extend(chunks);

        self.client
            .put_object_vectored(&key_str, parts)
            .await
            .map_err(|e| anyhow::anyhow!("object store put failed: {key_str}, {e:?}"))?;

        let etag = self
            .client
            .get_etag(&key_str)
            .await
            .unwrap_or_else(|_| "default_etag".to_string());
        let cache_key = format!("{}{}", key_str, etag);
        let _ = self.block_cache.remove(&cache_key).await;
        Ok(total_len as u64)
    }

    async fn write_range(&self, key: BlockKey, offset: u32, data: &[u8]) -> anyhow::Result<u64> {
        let key_str = Self::key_for(key);
        let mut buf = self
            .client
            .get_object(&key_str)
            .await
            .map_err(|e| anyhow::anyhow!("object store get failed: {:?}", e))?
            .unwrap_or_default();

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

    async fn write_fresh_range(
        &self,
        key: BlockKey,
        offset: u32,
        data: &[u8],
    ) -> anyhow::Result<u64> {
        let key_str = Self::key_for(key);
        if data.is_empty() {
            return Ok(0);
        }

        let offset_usize = offset as usize;
        let mut parts = Vec::new();
        if offset_usize > 0 {
            parts.extend(make_zero_bytes(offset_usize));
        }
        parts.push(Bytes::copy_from_slice(data));

        self.client
            .put_object_vectored(&key_str, parts)
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

    async fn read_range(&self, key: BlockKey, offset: u32, buf: &mut [u8]) -> anyhow::Result<()> {
        let key_str = Self::key_for(key);
        let len = buf.len();
        buf.fill(0);
        let start = offset as usize;
        let end = start + len;

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
            return Ok(());
        }

        let block = match self
            .client
            .get_object(&key_str)
            .await
            .map_err(|e| anyhow::anyhow!("object store get failed: {:?}", e))?
        {
            Some(data) => data,
            None => vec![0u8; end],
        };
        let copy_end = end.min(block.len());
        if copy_end > start {
            buf[..(copy_end - start)].copy_from_slice(&block[start..copy_end]);
        }
        let _ = self.block_cache.insert(&cache_key, &block).await;
        Ok(())
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

        let mut out = vec![0u8; data.len()];
        store
            .read_range((42, 3), layout.block_size / 4, &mut out)
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
        let mut data1 = vec![0u8; data.len()];
        store
            .read_range((42, 3), layout.block_size / 4, &mut data1)
            .await
            .unwrap();

        // Second read of the same data should hit the cache.
        let mut data2 = vec![0u8; data.len()];
        store
            .read_range((42, 3), layout.block_size / 4, &mut data2)
            .await
            .unwrap();
        assert_eq!(data1, data2);

        Ok(())
    }
}
