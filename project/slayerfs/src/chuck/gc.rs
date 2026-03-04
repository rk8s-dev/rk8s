//! Block Storage Garbage Collector
//!
//! This module provides garbage collection for block storage backends.

use crate::chuck::store::{BlockKey, BlockStore};
use crate::meta::store::{MetaError, MetaStore};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Configuration for block-level garbage collection.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BlockGcConfig {
    /// Interval between GC runs
    pub interval: Duration,
    /// Minimum age of delayed slices before deletion (safety period)
    pub min_age_secs: i64,
    /// Maximum number of slices to process per run
    pub batch_size: usize,
    /// Block size for calculating number of blocks to delete
    pub block_size: u64,
}

impl Default for BlockGcConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(3600),
            min_age_secs: 3600,
            batch_size: 1000,
            block_size: 4 * 1024 * 1024,
        }
    }
}

/// Garbage collector for block storage data.
#[allow(dead_code)]
pub struct BlockStoreGC<M, B> {
    meta_store: Arc<M>,
    block_store: Arc<B>,
}

#[allow(dead_code)]
impl<M, B> BlockStoreGC<M, B>
where
    M: MetaStore + Send + Sync + 'static,
    B: BlockStore + Send + Sync + 'static,
{
    pub fn new(meta_store: Arc<M>, block_store: Arc<B>) -> Self {
        Self {
            meta_store,
            block_store,
        }
    }

    pub fn start(self, config: BlockGcConfig) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = interval(config.interval);

            info!(
                interval_secs = config.interval.as_secs(),
                min_age_secs = config.min_age_secs,
                batch_size = config.batch_size,
                "BlockStore GC started"
            );

            loop {
                ticker.tick().await;

                if let Err(e) = self.run_gc_cycle(&config).await {
                    error!(error = %e, "GC cycle failed");
                }
            }
        })
    }

    pub async fn run_gc_cycle(&self, config: &BlockGcConfig) -> Result<(), GCError> {
        let deleted_slices = self
            .meta_store
            .process_delayed_slices(config.batch_size, config.min_age_secs)
            .await
            .map_err(GCError::MetaError)?;

        let deleted_count = deleted_slices.len();

        if deleted_count > 0 {
            info!(
                deleted_count = deleted_count,
                "GC cycle: processed delayed slices, deleting block data..."
            );

            for (slice_id, offset, size) in deleted_slices {
                if let Err(e) = self
                    .delete_slice_blocks(slice_id, offset, size, config.block_size)
                    .await
                {
                    warn!(
                        slice_id = slice_id,
                        offset = offset,
                        size = size,
                        error = %e,
                        "Failed to delete slice blocks from block store"
                    );
                }
            }

            info!(
                deleted_count = deleted_count,
                "GC cycle completed, deleted block data"
            );
        } else {
            debug!("GC cycle completed, no slices to process");
        }

        Ok(())
    }

    async fn delete_slice_blocks(
        &self,
        slice_id: u64,
        _offset: u64,
        size: u64,
        block_size: u64,
    ) -> Result<(), GCError> {
        if size == 0 {
            return Ok(());
        }

        // Blocks are indexed by (slice_id, block_index) where block_index is slice-relative
        // (starting from 0), not chunk-relative. The offset parameter is the slice's offset
        // within the chunk and should not be used for block indexing.
        let num_blocks = size.div_ceil(block_size);

        if num_blocks == 0 {
            return Ok(());
        }

        self.block_store
            .delete_range((slice_id, 0), num_blocks)
            .await
            .map_err(|e| {
                GCError::BlockStoreError(format!(
                    "Failed to delete blocks for slice {}: {}",
                    slice_id, e
                ))
            })?;

        Ok(())
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum GCError {
    MetaError(MetaError),
    BlockStoreError(String),
}

impl std::fmt::Display for GCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GCError::MetaError(e) => write!(f, "MetaStore error: {}", e),
            GCError::BlockStoreError(s) => write!(f, "BlockStore error: {}", s),
        }
    }
}

impl std::error::Error for GCError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GCError::MetaError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<MetaError> for GCError {
    fn from(e: MetaError) -> Self {
        GCError::MetaError(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::InMemoryBlockStore;
    use crate::chuck::slice::SliceDesc;
    use crate::meta::SLICE_ID_KEY;

    // ---- Mock MetaStore for GC testing ----

    struct MockMetaStore {
        /// Delayed slices returned by process_delayed_slices
        delayed: std::sync::Mutex<Vec<(u64, u64, u64)>>,
        /// Track how many times process_delayed_slices was called
        call_count: std::sync::atomic::AtomicU64,
        /// If set, process_delayed_slices returns this error
        fail_on_process: std::sync::Mutex<Option<MetaError>>,
    }

    impl MockMetaStore {
        fn new(delayed: Vec<(u64, u64, u64)>) -> Self {
            Self {
                delayed: std::sync::Mutex::new(delayed),
                call_count: std::sync::atomic::AtomicU64::new(0),
                fail_on_process: std::sync::Mutex::new(None),
            }
        }

        fn set_fail(&self, err: MetaError) {
            *self.fail_on_process.lock().unwrap() = Some(err);
        }
    }

    #[async_trait::async_trait]
    impl MetaStore for MockMetaStore {
        async fn process_delayed_slices(
            &self,
            batch_size: usize,
            _max_age_secs: i64,
        ) -> Result<Vec<(u64, u64, u64)>, MetaError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            if let Some(err) = self.fail_on_process.lock().unwrap().take() {
                return Err(err);
            }

            let mut guard = self.delayed.lock().unwrap();
            let n = batch_size.min(guard.len());
            let batch: Vec<_> = guard.drain(..n).collect();
            Ok(batch)
        }

        // ---- Stubs for trait completeness ----
        async fn get_slices(&self, _chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError> {
            Ok(vec![])
        }
        async fn next_id(&self, _key: &str) -> Result<i64, MetaError> {
            Ok(0)
        }
        async fn replace_slices_for_compact(
            &self,
            _chunk_id: u64,
            _new_slices: &[SliceDesc],
            _old_slices_to_delay: &[u8],
        ) -> Result<(), MetaError> {
            Ok(())
        }
        async fn stat(&self, _ino: i64) -> Result<Option<crate::meta::store::FileAttr>, MetaError> {
            Ok(None)
        }
        async fn lookup(&self, _parent: i64, _name: &str) -> Result<Option<i64>, MetaError> {
            Ok(None)
        }
        async fn lookup_path(
            &self,
            _path: &str,
        ) -> Result<Option<(i64, crate::meta::store::FileType)>, MetaError> {
            Ok(None)
        }
        async fn readdir(&self, _ino: i64) -> Result<Vec<crate::meta::store::DirEntry>, MetaError> {
            Ok(vec![])
        }
        async fn mkdir(&self, _parent: i64, _name: String) -> Result<i64, MetaError> {
            Ok(0)
        }
        async fn rmdir(&self, _parent: i64, _name: &str) -> Result<(), MetaError> {
            Ok(())
        }
        async fn create_file(&self, _parent: i64, _name: String) -> Result<i64, MetaError> {
            Ok(0)
        }
        async fn unlink(&self, _parent: i64, _name: &str) -> Result<(), MetaError> {
            Ok(())
        }
        async fn rename(
            &self,
            _old_parent: i64,
            _old_name: &str,
            _new_parent: i64,
            _new_name: String,
        ) -> Result<(), MetaError> {
            Ok(())
        }
        async fn rename_exchange(
            &self,
            _old_parent: i64,
            _old_name: &str,
            _new_parent: i64,
            _new_name: &str,
        ) -> Result<(), MetaError> {
            Ok(())
        }
        async fn set_file_size(&self, _ino: i64, _size: u64) -> Result<(), MetaError> {
            Ok(())
        }
        async fn get_names(&self, _ino: i64) -> Result<Vec<(Option<i64>, String)>, MetaError> {
            Ok(vec![])
        }
        async fn get_paths(&self, _ino: i64) -> Result<Vec<String>, MetaError> {
            Ok(vec![])
        }
        fn root_ino(&self) -> i64 {
            1
        }
        async fn initialize(&self) -> Result<(), MetaError> {
            Ok(())
        }
        async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError> {
            Ok(vec![])
        }
        async fn remove_file_metadata(&self, _ino: i64) -> Result<(), MetaError> {
            Ok(())
        }
        async fn append_slice(&self, _chunk_id: u64, _slice: SliceDesc) -> Result<(), MetaError> {
            Ok(())
        }
        async fn write(
            &self,
            _ino: i64,
            _chunk_id: u64,
            _slice: SliceDesc,
            _new_size: u64,
        ) -> Result<(), MetaError> {
            Ok(())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    // ---- delete_slice_blocks tests ----

    #[tokio::test]
    async fn test_delete_slice_blocks_zero_size() {
        let meta = Arc::new(MockMetaStore::new(vec![]));
        let store = Arc::new(InMemoryBlockStore::new());
        let gc = BlockStoreGC::new(meta, store);

        let result = gc.delete_slice_blocks(1, 0, 0, 4 * 1024 * 1024).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_slice_blocks_single_block() {
        let meta = Arc::new(MockMetaStore::new(vec![]));
        let store = Arc::new(InMemoryBlockStore::new());

        // Write a block at (slice_id=10, block_index=0)
        let data = vec![0xAA; 1024];
        store.write_fresh_range((10, 0), 0, &data).await.unwrap();

        let gc = BlockStoreGC::new(meta, store.clone());
        gc.delete_slice_blocks(10, 0, 1024, 4 * 1024 * 1024)
            .await
            .unwrap();

        // Verify block is deleted (read should return zeroes)
        let mut buf = vec![0u8; 1024];
        store.read_range((10, 0), 0, &mut buf).await.unwrap();
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[tokio::test]
    async fn test_delete_slice_blocks_multiple_blocks() {
        let meta = Arc::new(MockMetaStore::new(vec![]));
        let store = Arc::new(InMemoryBlockStore::new());
        let block_size = 1024u64;

        // Write 3 blocks
        for i in 0u32..3 {
            let data = vec![(i + 1) as u8; block_size as usize];
            store.write_fresh_range((5, i), 0, &data).await.unwrap();
        }

        let gc = BlockStoreGC::new(meta, store.clone());
        gc.delete_slice_blocks(5, 100, 3 * block_size, block_size)
            .await
            .unwrap();

        // All 3 blocks should be deleted
        for i in 0u32..3 {
            let mut buf = vec![0u8; block_size as usize];
            store.read_range((5, i), 0, &mut buf).await.unwrap();
            assert!(buf.iter().all(|&b| b == 0), "block {} not cleaned", i);
        }
    }

    /// Verify that offset is NOT used for block indexing — blocks always start from index 0.
    #[tokio::test]
    async fn test_delete_slice_blocks_offset_ignored() {
        let meta = Arc::new(MockMetaStore::new(vec![]));
        let store = Arc::new(InMemoryBlockStore::new());
        let block_size = 4096u64;

        // Write block at (slice_id=7, block_index=0)
        store
            .write_fresh_range((7, 0), 0, &vec![0xFF; block_size as usize])
            .await
            .unwrap();

        let gc = BlockStoreGC::new(meta, store.clone());
        // offset=99999 should be ignored — deletion starts at block_index=0
        gc.delete_slice_blocks(7, 99999, block_size, block_size)
            .await
            .unwrap();

        let mut buf = vec![0u8; block_size as usize];
        store.read_range((7, 0), 0, &mut buf).await.unwrap();
        assert!(
            buf.iter().all(|&b| b == 0),
            "block at index 0 should be deleted"
        );
    }

    /// Partial last block: div_ceil gives correct count.
    #[tokio::test]
    async fn test_delete_slice_blocks_partial_last_block() {
        let meta = Arc::new(MockMetaStore::new(vec![]));
        let store = Arc::new(InMemoryBlockStore::new());
        let block_size = 1024u64;

        // Write 2 blocks for slice 3
        for i in 0u32..2 {
            store
                .write_fresh_range((3, i), 0, &vec![0xBB; block_size as usize])
                .await
                .unwrap();
        }

        let gc = BlockStoreGC::new(meta, store.clone());
        // size = 1500, block_size = 1024 → 2 blocks (1024 + 476)
        gc.delete_slice_blocks(3, 0, 1500, block_size)
            .await
            .unwrap();

        for i in 0u32..2 {
            let mut buf = vec![0u8; block_size as usize];
            store.read_range((3, i), 0, &mut buf).await.unwrap();
            assert!(buf.iter().all(|&b| b == 0));
        }
    }

    // ---- run_gc_cycle tests ----

    #[tokio::test]
    async fn test_gc_cycle_no_delayed_slices() {
        let meta = Arc::new(MockMetaStore::new(vec![]));
        let store = Arc::new(InMemoryBlockStore::new());
        let gc = BlockStoreGC::new(meta.clone(), store);

        let config = BlockGcConfig {
            block_size: 1024,
            batch_size: 100,
            min_age_secs: 0,
            ..Default::default()
        };

        gc.run_gc_cycle(&config).await.unwrap();
        assert_eq!(meta.call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_gc_cycle_processes_slices() {
        let block_size = 1024u64;
        let store = Arc::new(InMemoryBlockStore::new());

        // Pre-populate block data for slice 42
        store
            .write_fresh_range((42, 0), 0, &vec![0xDD; block_size as usize])
            .await
            .unwrap();

        let meta = Arc::new(MockMetaStore::new(vec![
            (42, 0, block_size), // slice_id=42, offset=0, size=1024
        ]));

        let gc = BlockStoreGC::new(meta, store.clone());
        let config = BlockGcConfig {
            block_size,
            batch_size: 100,
            min_age_secs: 0,
            ..Default::default()
        };

        gc.run_gc_cycle(&config).await.unwrap();

        // Block data should be cleaned
        let mut buf = vec![0u8; block_size as usize];
        store.read_range((42, 0), 0, &mut buf).await.unwrap();
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[tokio::test]
    async fn test_gc_cycle_batch_respects_limit() {
        let meta = Arc::new(MockMetaStore::new(vec![
            (1, 0, 100),
            (2, 0, 200),
            (3, 0, 300),
            (4, 0, 400),
        ]));
        let store = Arc::new(InMemoryBlockStore::new());
        let gc = BlockStoreGC::new(meta.clone(), store);

        let config = BlockGcConfig {
            block_size: 1024,
            batch_size: 2, // Only process 2 per cycle
            min_age_secs: 0,
            ..Default::default()
        };

        gc.run_gc_cycle(&config).await.unwrap();

        // After one cycle, only 2 should have been consumed
        let remaining = meta.delayed.lock().unwrap().len();
        assert_eq!(remaining, 2, "batch_size=2 means 2 left after first cycle");
    }

    #[tokio::test]
    async fn test_gc_cycle_meta_error_propagated() {
        let meta = Arc::new(MockMetaStore::new(vec![]));
        meta.set_fail(MetaError::NotImplemented);

        let store = Arc::new(InMemoryBlockStore::new());
        let gc = BlockStoreGC::new(meta, store);

        let config = BlockGcConfig::default();
        let result = gc.run_gc_cycle(&config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_gc_cycle_multiple_rounds() {
        let block_size = 512u64;
        let store = Arc::new(InMemoryBlockStore::new());

        // 4 slices, each with 1 block
        for sid in 1..=4u64 {
            store
                .write_fresh_range((sid, 0), 0, &vec![0xFF; block_size as usize])
                .await
                .unwrap();
        }

        let meta = Arc::new(MockMetaStore::new(vec![
            (1, 0, block_size),
            (2, 0, block_size),
            (3, 0, block_size),
            (4, 0, block_size),
        ]));

        let gc = BlockStoreGC::new(meta.clone(), store.clone());
        let config = BlockGcConfig {
            block_size,
            batch_size: 2,
            min_age_secs: 0,
            ..Default::default()
        };

        // First round: slices 1, 2
        gc.run_gc_cycle(&config).await.unwrap();
        // Second round: slices 3, 4
        gc.run_gc_cycle(&config).await.unwrap();

        // All blocks should be deleted
        for sid in 1..=4u64 {
            let mut buf = vec![0u8; block_size as usize];
            store.read_range((sid, 0), 0, &mut buf).await.unwrap();
            assert!(buf.iter().all(|&b| b == 0), "slice {} not cleaned", sid);
        }
    }

    // ---- Error type tests ----

    #[test]
    fn test_gc_error_display() {
        let e1 = GCError::MetaError(MetaError::NotImplemented);
        assert!(e1.to_string().contains("MetaStore error"));

        let e2 = GCError::BlockStoreError("timeout".into());
        assert!(e2.to_string().contains("BlockStore error"));
        assert!(e2.to_string().contains("timeout"));
    }

    #[test]
    fn test_gc_error_source() {
        let e = GCError::MetaError(MetaError::NotImplemented);
        assert!(std::error::Error::source(&e).is_some());

        let e = GCError::BlockStoreError("x".into());
        assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn test_gc_error_from_meta() {
        let _: GCError = MetaError::NotImplemented.into();
    }

    #[test]
    fn test_block_gc_config_default() {
        let cfg = BlockGcConfig::default();
        assert_eq!(cfg.interval, Duration::from_secs(3600));
        assert_eq!(cfg.min_age_secs, 3600);
        assert_eq!(cfg.batch_size, 1000);
        assert_eq!(cfg.block_size, 4 * 1024 * 1024);
    }
}
