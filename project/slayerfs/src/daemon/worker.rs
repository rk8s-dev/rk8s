//! Background workers (upload, gc)
//!
//! Note: Background compaction is handled by `chunk::compact::worker::CompactionWorker`.

use crate::cadapter::client::{ObjectBackend, ObjectClient};
use crate::chunk::ChunkLayout;
use crate::chunk::store::BlockStore;
use crate::meta::store::MetaStore;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

#[allow(dead_code)]
pub(crate) fn start_upload_workers() {
    // TODO: implement upload worker pool
}

/// Configuration for object-level garbage collection.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ObjectGcConfig {
    /// GC run interval (seconds)
    pub interval_secs: u64,
    /// GC run batch size
    pub batch_size: usize,
    /// Minimum age of delayed slices to delete (seconds)
    pub max_age_secs: i64,
    pub layout: ChunkLayout,
}

impl Default for ObjectGcConfig {
    fn default() -> Self {
        Self {
            interval_secs: 3600,
            batch_size: 100,
            max_age_secs: 3600,
            layout: ChunkLayout::default(),
        }
    }
}

#[allow(dead_code)]
pub(crate) struct MarkBasedGarbageCollector<B: ObjectBackend + Clone> {
    meta_store: Arc<dyn MetaStore>,
    object_client: Arc<ObjectClient<B>>,
    block_store: Arc<dyn BlockStore + Send + Sync>,
    config: ObjectGcConfig,
}

#[allow(dead_code)]
impl<B: ObjectBackend + Clone> MarkBasedGarbageCollector<B> {
    pub(crate) fn new(
        meta_store: Arc<dyn MetaStore>,
        object_client: Arc<ObjectClient<B>>,
        block_store: Arc<dyn BlockStore + Send + Sync>,
        config: ObjectGcConfig,
    ) -> Self {
        Self {
            meta_store,
            object_client,
            block_store,
            config,
        }
    }

    pub(crate) async fn start(&self) {
        let mut ticker = interval(Duration::from_secs(self.config.interval_secs));

        info!("GC interval {} seconds", self.config.interval_secs);

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                info!("Received Ctrl+C, shutting down GC gracefully");
                let _ = shutdown_tx.send(());
            }
        });

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match self.run_gc_cycle().await {
                        Ok((delayed_deleted, deleted_files, deleted_objects)) => {
                            if delayed_deleted > 0 || deleted_files > 0 || deleted_objects > 0 {
                                info!(
                                    "GC interval {} seconds completed, processed {} delayed slices, cleaned up {} deleted files and {} data objects",
                                    self.config.interval_secs, delayed_deleted, deleted_files, deleted_objects
                                );
                            } else {
                                debug!("GC interval completed, no items to clean up");
                            }
                        }
                        Err(e) => {
                            error!("GC interval execution failed: {}", e);
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("GC shutting down gracefully");
                    break;
                }
            }
        }

        info!("GC stopped");
    }

    async fn run_gc_cycle(
        &self,
    ) -> Result<(usize, usize, usize), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting GC cycle");

        // Phase 1: Process delayed slices (from compaction) and delete their block data
        let pending_deletions = self
            .meta_store
            .process_delayed_slices(self.config.batch_size, self.config.max_age_secs)
            .await?;
        let delayed_deleted_count = pending_deletions.len();

        if delayed_deleted_count > 0 {
            info!(
                deleted_count = delayed_deleted_count,
                "GC cycle: processed delayed slices, deleting block data..."
            );

            let mut confirmed_ids = Vec::new();

            for (slice_id, _offset, size, delayed_id) in &pending_deletions {
                if let Err(e) = self
                    .delete_slice_blocks(*slice_id, *size, self.config.layout.block_size as u64)
                    .await
                {
                    warn!(
                        slice_id = slice_id,
                        size = size,
                        error = %e,
                        "Failed to delete slice blocks from block store, will retry later"
                    );
                } else {
                    confirmed_ids.push(*delayed_id);
                }
            }

            // Phase 2: Confirm successful deletions
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
                deleted_count = delayed_deleted_count,
                confirmed_count = confirmed_ids.len(),
                "GC cycle: deleted block data for delayed slices"
            );
        }

        let deleted_inodes = self.meta_store.get_deleted_files().await?;
        debug!("Identified {} deleted file inodes", deleted_inodes.len());

        if deleted_inodes.is_empty() {
            return Ok((delayed_deleted_count, 0, 0));
        }

        let deleted_objects = self.delete_objects(&deleted_inodes).await?;
        self.cleanup_deleted_file_metadata(&deleted_inodes).await?;

        Ok((delayed_deleted_count, deleted_inodes.len(), deleted_objects))
    }

    async fn delete_slice_blocks(
        &self,
        slice_id: u64,
        size: u64,
        block_size: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if size == 0 {
            return Ok(());
        }

        // Blocks are indexed by (slice_id, block_index) where block_index is slice-relative
        let num_blocks = size.div_ceil(block_size);

        if num_blocks == 0 {
            return Ok(());
        }

        self.block_store
            .delete_range((slice_id, 0), num_blocks)
            .await?;

        Ok(())
    }

    async fn delete_objects(
        &self,
        deleted_inodes: &[i64],
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let mut deleted_objects: usize = 0;

        for &inode in deleted_inodes {
            debug!("Deleting object for inode {}", inode);

            if let Some(stat) = self.meta_store.stat(inode).await? {
                let chunk_count = stat.size.div_ceil(self.config.layout.chunk_size);
                let blocks_per_chunk = self.config.layout.blocks_per_chunk();

                for chunk_id in 0..chunk_count {
                    for block_index in 0..blocks_per_chunk {
                        let key = format!("chunks/{chunk_id}/{block_index}");
                        self.object_client.delete_object(&key).await?;
                        deleted_objects += 1;
                    }
                }
            }
        }

        Ok(deleted_objects)
    }

    async fn cleanup_deleted_file_metadata(
        &self,
        deleted_inodes: &[i64],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "cleaning up {} deleted file metadata records",
            deleted_inodes.len()
        );

        for &inode in deleted_inodes {
            debug!("cleaning up file metadata for inode {}", inode);
            self.meta_store.remove_file_metadata(inode).await?;
        }

        Ok(())
    }
}

#[allow(dead_code)]
pub async fn start_gc<B: ObjectBackend + Clone>(
    meta_store: Arc<dyn MetaStore>,
    object_client: Arc<ObjectClient<B>>,
    block_store: Arc<dyn BlockStore + Send + Sync>,
    config: Option<ObjectGcConfig>,
) {
    let config = config.unwrap_or_default();
    let gc = MarkBasedGarbageCollector::new(meta_store, object_client, block_store, config);
    gc.start().await;
}
