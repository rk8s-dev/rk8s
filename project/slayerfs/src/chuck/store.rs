//! Storage backends: asynchronous block-level IO traits and in-memory implementations.

use crate::chuck::singleflight::SingleFlight;
use crate::utils::NumCastExt;
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

/// Abstract block store interface (cadapter/S3/etc. can implement this).
#[async_trait]
// ensure offset_in_block + data.len() <= block_size
pub trait BlockStore {
    async fn write_range(&self, key: BlockKey, offset: u64, data: &[u8]) -> anyhow::Result<u64>;

    /// Write a new block without reading any existing data.
    ///
    /// This exists to support COW-style writes where every write targets a fresh object/key.
    /// In that model, read-modify-write is wasted IO because there is no old data to preserve.
    /// Callers must ensure the target key is fresh; using this on an existing object would
    /// drop any previous content outside the written range.
    #[tracing::instrument(level = "trace", skip(self, chunks), fields(key = ?key, offset, chunk_count = chunks.len()))]
    async fn write_fresh_vectored(
        &self,
        key: BlockKey,
        offset: u64,
        chunks: Vec<Bytes>,
    ) -> anyhow::Result<u64> {
        let data = chunks
            .into_iter()
            .flat_map(|e| e.to_vec())
            .collect::<Vec<_>>();
        self.write_fresh_range(key, offset, &data).await
    }

    /// Write a new block without reading any existing data. See write_fresh_vectored for details.
    async fn write_fresh_range(
        &self,
        key: BlockKey,
        offset: u64,
        data: &[u8],
    ) -> anyhow::Result<u64> {
        self.write_range(key, offset, data).await
    }

    async fn read_range(&self, key: BlockKey, offset: u64, buf: &mut [u8]) -> anyhow::Result<()>;

    #[allow(dead_code)]
    async fn delete_range(&self, key: BlockKey, len: u64) -> anyhow::Result<()>;
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
    async fn write_range(&self, key: BlockKey, offset: u64, data: &[u8]) -> anyhow::Result<u64> {
        let mut guard = self.map.write().await;
        let entry = guard.entry(key).or_insert_with(Vec::new);
        let start = offset.as_usize();
        let end = start + data.len();
        if entry.len() < end {
            entry.resize(end, 0);
        }
        entry[start..end].copy_from_slice(data);
        Ok(data.len() as u64)
    }

    // Caller is responsible for zero-filling buf; this method only overwrites existing bytes.
    async fn read_range(&self, key: BlockKey, offset: u64, buf: &mut [u8]) -> anyhow::Result<()> {
        let guard = self.map.read().await;
        if let Some(src) = guard.get(&key) {
            let start = offset.as_usize();
            let end = start + buf.len();
            let copy_end = end.min(src.len());
            if copy_end > start {
                let len = copy_end - start;
                buf[..len].copy_from_slice(&src[start..copy_end]);
            }
        }
        Ok(())
    }

    async fn delete_range(&self, key: BlockKey, len: u64) -> anyhow::Result<()> {
        let (chunk_id, block_index) = key;
        let mut guard = self.map.write().await;
        let start = block_index;
        let end = start + len.as_u32();
        for i in start..end {
            guard.remove(&(chunk_id, i));
        }
        Ok(())
    }
}

/// BlockStore backed by cadapter::client (key space `chunks/{chunk_id}/{block_index}`).
pub struct ObjectBlockStore<B: ObjectBackend> {
    client: ObjectClient<B>,
    #[allow(dead_code)]
    block_cache: ChunksCache,
    /// SingleFlight controller for coalescing concurrent reads to the same block
    read_flight: SingleFlight<BlockKey, Bytes>,
    /// Configuration for read strategy
    config: BlockStoreConfig,
}

/// Configuration for ObjectBlockStore read strategy
#[derive(Debug, Clone)]
pub struct BlockStoreConfig {
    /// Block size in bytes (default: 4MB)
    pub block_size: usize,
    /// For ranges smaller than this threshold, use direct range read instead of full block read
    /// Default is 25% of block size (1MB for 4MB blocks)
    pub range_read_threshold: f32,
}

impl Default for BlockStoreConfig {
    fn default() -> Self {
        Self {
            block_size: 4 * 1024 * 1024, // 4MB
            range_read_threshold: 0.25,  // 25% = 1MB for 4MB blocks
        }
    }
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
            read_flight: SingleFlight::new(),
            config: BlockStoreConfig::default(),
        }
    }
    /// Creates a new ObjectBlockStore with custom cache configuration
    #[allow(unused)]
    pub fn new_with_config(
        client: ObjectClient<B>,
        cache_config: ChunksCacheConfig,
    ) -> anyhow::Result<Self> {
        Self::new_with_configs(client, cache_config, BlockStoreConfig::default())
    }

    /// Creates a new ObjectBlockStore with custom cache and block store configurations
    #[allow(unused)]
    pub fn new_with_configs(
        client: ObjectClient<B>,
        cache_config: ChunksCacheConfig,
        store_config: BlockStoreConfig,
    ) -> anyhow::Result<Self> {
        let cache_dir = dirs::cache_dir().unwrap().join("slayerfs");
        let _ = fs::create_dir_all(cache_dir.clone());

        let block_cache = block_on(ChunksCache::new_with_config(cache_config))
            .map_err(|e| anyhow::anyhow!("Failed to create cache: {}", e))?;
        Ok(Self {
            client,
            block_cache,
            read_flight: SingleFlight::new(),
            config: store_config,
        })
    }

    fn key_for(key: BlockKey) -> String {
        let (chunk_id, block_index) = key;
        format!("chunks/{chunk_id}/{block_index}")
    }
}

#[async_trait]
impl<B: ObjectBackend + Send + Sync> BlockStore for ObjectBlockStore<B> {
    async fn write_range(&self, key: BlockKey, offset: u64, data: &[u8]) -> anyhow::Result<u64> {
        let key_str = Self::key_for(key);
        let mut buf = self
            .client
            .get_object(&key_str)
            .await
            .map_err(|e| anyhow::anyhow!("object store get failed: {:?}", e))?
            .unwrap_or_default();

        let start = offset.as_usize();
        let end = start + data.len();
        if buf.len() < end {
            buf.resize(end, 0);
        }
        buf[start..end].copy_from_slice(data);
        self.client
            .put_object(&key_str, &buf)
            .await
            .map_err(|e| anyhow::anyhow!("object store put failed: {key_str}, {e:?}"))?;

        Ok(data.len() as u64)
    }

    #[tracing::instrument(name = "ObjectBlockStore.write_fresh_vectored", level = "trace", skip(self, chunks), fields(key = ?key, offset, chunk_count = chunks.len()))]
    async fn write_fresh_vectored(
        &self,
        key: BlockKey,
        offset: u64,
        chunks: Vec<Bytes>,
    ) -> anyhow::Result<u64> {
        let key_str = Self::key_for(key);
        let total_len = chunks.iter().map(|c| c.len()).sum::<usize>();
        if total_len == 0 {
            return Ok(0);
        }

        let offset_usize = offset.as_usize();
        let mut parts: Vec<Bytes> = Vec::new();
        if offset_usize > 0 {
            parts.extend(make_zero_bytes(offset_usize));
        }
        parts.extend(chunks);

        self.client
            .put_object_vectored(&key_str, parts)
            .await
            .map_err(|e| anyhow::anyhow!("object store put failed: {key_str}, {e:?}"))?;

        Ok(total_len as u64)
    }

    async fn write_fresh_range(
        &self,
        key: BlockKey,
        offset: u64,
        data: &[u8],
    ) -> anyhow::Result<u64> {
        let key_str = Self::key_for(key);
        if data.is_empty() {
            return Ok(0);
        }

        let offset_usize = offset.as_usize();
        let mut parts = Vec::new();
        if offset_usize > 0 {
            parts.extend(make_zero_bytes(offset_usize));
        }
        parts.push(Bytes::copy_from_slice(data));

        self.client
            .put_object_vectored(&key_str, parts)
            .await
            .map_err(|e| anyhow::anyhow!("object store put failed: {key_str}, {e:?}"))?;

        Ok(data.len() as u64)
    }

    #[tracing::instrument(
        name = "ObjectBlockStore.read_range",
        level = "trace",
        skip(self, buf),
        fields(key = ?key, offset, len = buf.len(), read_len = tracing::field::Empty, strategy = tracing::field::Empty)
    )]
    // Caller is responsible for zero-filling buf; this method only overwrites existing bytes.
    async fn read_range(&self, key: BlockKey, offset: u64, buf: &mut [u8]) -> anyhow::Result<()> {
        let len = buf.len();
        let range_size_threshold =
            (self.config.block_size as f32 * self.config.range_read_threshold) as usize;

        // Smart strategy selection:
        // 1. If the requested range is small (< threshold), use direct range read
        // 2. If the range is large, use SingleFlight to potentially coalesce with other requests
        if len < range_size_threshold {
            // Strategy 1: Direct range read for small ranges (efficient for random access)
            tracing::Span::current().record("strategy", "direct_range");

            let key_str = Self::key_for(key);
            let read_len = self
                .client
                .get_object_range(&key_str, offset, buf)
                .await
                .map_err(|e| anyhow::anyhow!("object store range read failed: {key_str}, {e:?}"))?;

            tracing::Span::current().record("read_len", read_len);
            return Ok(());
        }

        // Strategy 2: Full block read with SingleFlight (efficient for large reads and concurrent access)
        tracing::Span::current().record("strategy", "coalesced_full");

        // Use SingleFlight to coalesce concurrent reads to the same block.
        // We read the entire block and then extract the requested range.
        let client = &self.client;
        let block_data =
            self.read_flight
                .execute(key, || async move {
                    // Read the entire block
                    let key_str = Self::key_for(key);
                    let data = client.get_object(&key_str).await.map_err(|e| {
                        anyhow::anyhow!("object store get failed: {key_str}, {e:?}")
                    })?;

                    Ok::<_, anyhow::Error>(Bytes::from(data.unwrap_or_default()))
                })
                .await
                .map_err(|e| anyhow::anyhow!("SingleFlight read failed: {e}"))?;

        // Extract the requested range from the block data
        let offset_usize = offset as usize;
        let end = offset_usize + len;

        if offset_usize < block_data.len() {
            let copy_end = end.min(block_data.len());
            let copy_len = copy_end - offset_usize;
            buf[..copy_len].copy_from_slice(&block_data.as_ref()[offset_usize..copy_end]);
            tracing::Span::current().record("read_len", copy_len);
        } else {
            tracing::Span::current().record("read_len", 0_usize);
        }

        Ok(())
    }

    async fn delete_range(&self, key: BlockKey, len: u64) -> anyhow::Result<()> {
        let (chunk_id, block_index) = key;
        let start = block_index;
        let end = start + len.as_u32();
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
            .write_range((42, 3), (layout.block_size / 4) as u64, &data)
            .await
            .unwrap();

        let mut out = vec![0u8; data.len()];
        store
            .read_range((42, 3), (layout.block_size / 4) as u64, &mut out)
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
            .write_range((42, 3), (layout.block_size / 4) as u64, &data)
            .await
            .unwrap();
        // First read should miss the cache.
        let mut data1 = vec![0u8; data.len()];
        store
            .read_range((42, 3), (layout.block_size / 4) as u64, &mut data1)
            .await
            .unwrap();

        // Second read of the same data should hit the cache.
        let mut data2 = vec![0u8; data.len()];
        store
            .read_range((42, 3), (layout.block_size / 4) as u64, &mut data2)
            .await
            .unwrap();
        assert_eq!(data1, data2);

        Ok(())
    }

    #[tokio::test]
    async fn test_intelligent_read_strategy() -> Result<(), Box<dyn std::error::Error>> {
        use crate::cadapter::client::{ObjectBackend, ObjectClient};
        use async_trait::async_trait;
        use std::{
            collections::HashMap,
            sync::{Arc, Mutex},
        };

        #[derive(Debug, Clone)]
        struct MockStats {
            get_object_calls: usize,
            get_object_range_calls: usize,
        }

        #[derive(Clone)]
        struct MockBackend {
            data: Arc<Mutex<HashMap<String, Vec<u8>>>>,
            stats: Arc<Mutex<MockStats>>,
        }

        impl MockBackend {
            fn new() -> Self {
                let mut data = HashMap::new();
                // Create a 4MB block with known pattern
                let block_data: Vec<u8> = (0..4_194_304).map(|i| (i % 256) as u8).collect();
                data.insert("chunks/42/3".to_string(), block_data);

                Self {
                    data: Arc::new(Mutex::new(data)),
                    stats: Arc::new(Mutex::new(MockStats {
                        get_object_calls: 0,
                        get_object_range_calls: 0,
                    })),
                }
            }

            fn get_stats(&self) -> MockStats {
                self.stats.lock().unwrap().clone()
            }

            fn reset_stats(&self) {
                let mut stats = self.stats.lock().unwrap();
                stats.get_object_calls = 0;
                stats.get_object_range_calls = 0;
            }
        }

        #[async_trait]
        impl ObjectBackend for MockBackend {
            async fn put_object(&self, key: &str, data: &[u8]) -> anyhow::Result<()> {
                self.data
                    .lock()
                    .unwrap()
                    .insert(key.to_string(), data.to_vec());
                Ok(())
            }

            async fn get_object(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
                self.stats.lock().unwrap().get_object_calls += 1;
                Ok(self.data.lock().unwrap().get(key).cloned())
            }

            async fn get_object_range(
                &self,
                key: &str,
                offset: u64,
                buf: &mut [u8],
            ) -> anyhow::Result<usize> {
                self.stats.lock().unwrap().get_object_range_calls += 1;
                if let Some(data) = self.data.lock().unwrap().get(key) {
                    let offset = offset as usize;
                    let end = (offset + buf.len()).min(data.len());
                    if offset < data.len() {
                        let copy_len = end - offset;
                        buf[..copy_len].copy_from_slice(&data[offset..end]);
                        Ok(copy_len)
                    } else {
                        Ok(0)
                    }
                } else {
                    Ok(0)
                }
            }

            async fn get_etag(&self, _key: &str) -> anyhow::Result<String> {
                Ok("test_etag".to_string())
            }

            async fn delete_object(&self, key: &str) -> anyhow::Result<()> {
                self.data.lock().unwrap().remove(key);
                Ok(())
            }
        }

        // Test small range uses direct read
        let backend = MockBackend::new();
        let client = ObjectClient::new(backend.clone());
        let config = BlockStoreConfig {
            block_size: 4 * 1024 * 1024,
            range_read_threshold: 0.25, // 1MB threshold
        };
        let store =
            ObjectBlockStore::new_with_configs(client, ChunksCacheConfig::default(), config)?;

        backend.reset_stats();

        // Small read (512KB < 1MB threshold) - should use range read
        let mut small_buf = vec![0u8; 512 * 1024];
        store.read_range((42, 3), 0, &mut small_buf).await?;

        let stats = backend.get_stats();
        assert_eq!(
            stats.get_object_range_calls, 1,
            "Small read should use range read"
        );
        assert_eq!(
            stats.get_object_calls, 0,
            "Small read should not use full read"
        );

        backend.reset_stats();

        // Large read (2MB > 1MB threshold) - should use full read
        let mut large_buf = vec![0u8; 2 * 1024 * 1024];
        store.read_range((42, 3), 0, &mut large_buf).await?;

        let stats = backend.get_stats();
        assert_eq!(stats.get_object_calls, 1, "Large read should use full read");
        assert_eq!(
            stats.get_object_range_calls, 0,
            "Large read should not use range read"
        );

        println!("✅ 智能读取策略测试通过:");
        println!("   - 小范围读取 (512KB) → 使用 get_object_range");
        println!("   - 大范围读取 (2MB) → 使用 get_object + SingleFlight");

        Ok(())
    }
}
