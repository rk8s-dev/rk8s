use crate::chunk::store::{BlockKey, BlockStore};
use crate::meta::store::{MetaError, MetaStore};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct BlockGcConfig {
    pub interval: Duration,
    pub min_age_secs: i64,
    pub batch_size: usize,
    pub block_size: u64,
    pub orphan_cleanup_age_secs: i64,
}

impl Default for BlockGcConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(3600),
            min_age_secs: 3600,
            batch_size: 1000,
            block_size: 4 * 1024 * 1024,
            orphan_cleanup_age_secs: 3600,
        }
    }
}

pub struct BlockStoreGC<M: ?Sized, B> {
    meta_store: Arc<M>,
    block_store: Arc<B>,
}

impl<M: ?Sized, B> BlockStoreGC<M, B>
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
        let pending_deletions = self
            .meta_store
            .process_delayed_slices(config.batch_size, config.min_age_secs)
            .await
            .map_err(GCError::MetaError)?;

        let deleted_count = pending_deletions.len();

        if deleted_count > 0 {
            info!(
                deleted_count = deleted_count,
                "GC cycle: processed delayed slices, deleting block data..."
            );

            let mut confirmed_ids = Vec::new();

            for (slice_id, offset, size, delayed_id) in pending_deletions {
                if let Err(e) = self
                    .delete_slice_blocks(slice_id, offset, size, config.block_size)
                    .await
                {
                    warn!(
                        slice_id = slice_id,
                        offset = offset,
                        size = size,
                        error = %e,
                        "Failed to delete slice blocks from block store, will retry later"
                    );
                } else {
                    confirmed_ids.push(delayed_id);
                }
            }

            if !confirmed_ids.is_empty()
                && let Err(e) = self
                    .meta_store
                    .confirm_delayed_deleted(&confirmed_ids)
                    .await
            {
                warn!(
                    count = confirmed_ids.len(),
                    error = %e,
                    "Failed to confirm delayed deletions"
                );
            }

            info!(
                deleted_count = deleted_count,
                confirmed_count = confirmed_ids.len(),
                "GC cycle: delayed slices processed"
            );
        }

        match self
            .meta_store
            .cleanup_orphan_uncommitted_slices(config.orphan_cleanup_age_secs, config.batch_size)
            .await
        {
            Ok(orphans) if !orphans.is_empty() => {
                let mut cleaned_slice_ids = Vec::new();
                for (slice_id, size) in &orphans {
                    let block_count = size.div_ceil(config.block_size);
                    if let Err(e) = self
                        .block_store
                        .delete_range((*slice_id, 0), block_count)
                        .await
                    {
                        warn!(
                            slice_id = slice_id,
                            error = %e,
                            "Failed to delete orphan slice blocks, will retry later"
                        );
                    } else {
                        debug!(slice_id = slice_id, "Deleted orphan slice blocks");
                        cleaned_slice_ids.push(*slice_id);
                    }
                }
                if let Err(e) = self
                    .meta_store
                    .delete_uncommitted_slices(&cleaned_slice_ids)
                    .await
                {
                    warn!(
                        count = cleaned_slice_ids.len(),
                        error = %e,
                        "Failed to delete uncommitted slice records, will retry in next GC cycle"
                    );
                } else {
                    info!(
                        count = cleaned_slice_ids.len(),
                        "Cleaned up orphan uncommitted slice records"
                    );
                }
            }
            Ok(_) => {
                debug!("GC cycle: no orphan uncommitted slices found");
            }
            Err(e) => {
                warn!(error = %e, "Failed to cleanup orphan uncommitted slices");
            }
        }

        if deleted_count == 0 {
            debug!("GC cycle completed, no slices to process");
        }

        Ok(())
    }

    async fn delete_slice_blocks(
        &self,
        slice_id: u64,
        _chunk_offset: u64,
        size: u64,
        block_size: u64,
    ) -> Result<(), GCError> {
        if size == 0 {
            return Ok(());
        }

        // Block keys are (slice_id, slice_relative_block_index), so deletion always
        // starts from block index 0 for the target slice. chunk_offset is kept in
        // metadata for audit/replay context and is intentionally not used here.
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

#[derive(Debug, Error)]
pub enum GCError {
    #[error("MetaStore error: {0}")]
    MetaError(#[from] MetaError),
    #[error("BlockStore error: {0}")]
    BlockStoreError(String),
}
