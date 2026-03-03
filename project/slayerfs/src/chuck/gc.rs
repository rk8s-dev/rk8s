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
