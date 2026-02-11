//! Background workers (upload, compaction, gc)

use crate::cadapter::client::{ObjectBackend, ObjectClient};
use crate::chuck::ChunkLayout;
use crate::meta::store::MetaStore;
use std::sync::Arc;
use tokio::time::{Duration, interval};
use tracing::{debug, error, info};

/// Compact worker configuration
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CompactWorkerConfig {
    /// Compact check interval
    pub interval_secs: u64,
    /// Max chunks to process per run
    pub max_chunks_per_run: usize,
}

impl Default for CompactWorkerConfig {
    fn default() -> Self {
        Self {
            interval_secs: 600, // seconds
            max_chunks_per_run: 1000,
        }
    }
}

/// Compact worker for background slice compaction
#[allow(dead_code)]
pub(crate) struct CompactWorker {
    meta_store: Arc<dyn MetaStore>,
    config: CompactWorkerConfig,
}

#[allow(dead_code)]
impl CompactWorker {
    pub(crate) fn new(
        meta_store: Arc<dyn MetaStore>,
        config: CompactWorkerConfig,
    ) -> Self {
        Self {
            meta_store,
            config,
        }
    }

    /// start the compact worker with graceful shutdown support
    pub(crate) async fn start(&self) {
        let mut interval = interval(Duration::from_secs(self.config.interval_secs));

        // info!("Compact worker started, interval {} seconds", self.config.interval_secs);

        // shutdown signal handling
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        // setup Ctrl+C handler
        tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                info!("Received Ctrl+C, shutting down compact worker gracefully");
                let _ = shutdown_tx.send(());
            }
        });

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.run_compact_cycle().await {
                        Ok(compacted_count) => {
                            if compacted_count > 0 {
                                info!(
                                    "Compact cycle completed, compacted {} chunks",
                                    compacted_count
                                );
                            } else {
                                debug!("Compact cycle completed, no chunks needed compaction");
                            }
                        }
                        Err(e) => {
                            error!("Compact cycle execution failed: {}", e);
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Compact worker shutting down gracefully");
                    break;
                }
            }
        }

        info!("Compact worker stopped");
    }

    /// execute a full compact cycle
    async fn run_compact_cycle(
        &self,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        debug!("Starting compact cycle");

        // run compaction on chunks that meet the threshold criteria
        let compacted_count = self.meta_store.run_compact_by_threshold().await?;

        Ok(compacted_count)
    }
}

/// Start compact worker with graceful shutdown support
#[allow(dead_code)]
pub async fn start_compact_worker(
    meta_store: Arc<dyn MetaStore>,
    config: Option<CompactWorkerConfig>,
) {
    let config = config.unwrap_or_default();
    let worker = CompactWorker::new(meta_store, config);

    worker.start().await;
}

#[allow(dead_code)]
pub(crate) fn start_upload_workers() {
    // TODO: implement upload worker pool
}

/// Garbage collection configuration
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GcConfig {
    /// GC run interval (seconds)
    pub interval_secs: u64,
    /// GC run batch size
    pub batch_size: usize,

    pub layout: ChunkLayout,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            interval_secs: 3600,
            batch_size: 100,
            layout: ChunkLayout::default(),
        }
    }
}

#[allow(dead_code)]
pub(crate) struct MarkBasedGarbageCollector<B: ObjectBackend> {
    meta_store: Arc<dyn MetaStore>,
    object_client: Arc<ObjectClient<B>>,
    config: GcConfig,
}

#[allow(dead_code)]
impl<B: ObjectBackend> MarkBasedGarbageCollector<B> {
    pub(crate) fn new(
        meta_store: Arc<dyn MetaStore>,
        object_client: Arc<ObjectClient<B>>,
        config: GcConfig,
    ) -> Self {
        Self {
            meta_store,
            object_client,
            config,
        }
    }

    /// Start the garbage collector with graceful shutdown support
    pub(crate) async fn start(&self) {
        let mut interval = interval(Duration::from_secs(self.config.interval_secs));

        info!("GC interval {} seconds", self.config.interval_secs);

        // Simple shutdown signal handling
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        // Setup Ctrl+C handler
        tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                info!("Received Ctrl+C, shutting down GC gracefully");
                let _ = shutdown_tx.send(());
            }
        });

        loop {
            tokio::select! {
                _ = interval.tick() => {
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

    /// Execute a full garbage collection cycle
    async fn run_gc_cycle(
        &self,
    ) -> Result<(usize, usize, usize), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting GC cycle");

        // process delayed slices, soft deleted slices that are now safe to delete
        let delayed_deleted = self.meta_store.process_delayed_slices(self.config.batch_size).await?;
        if delayed_deleted > 0 {
            info!("Processed {} delayed slices", delayed_deleted);
        }

        // get deleted file inodes
        let deleted_inodes = self.meta_store.get_deleted_files().await?;
        debug!("Identified {} deleted file inodes", deleted_inodes.len());

        if deleted_inodes.is_empty() {
            return Ok((delayed_deleted, 0, 0));
        }

        let deleted_objects = self.delete_objects(&deleted_inodes).await?;

        self.cleanup_deleted_file_metadata(&deleted_inodes).await?;

        Ok((delayed_deleted, deleted_inodes.len(), deleted_objects))
    }

    /// batch delete objects
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

/// Start garbage collector with graceful shutdown support
#[allow(dead_code)]
pub async fn start_gc<B: ObjectBackend>(
    meta_store: Arc<dyn MetaStore>,
    object_client: Arc<ObjectClient<B>>,
    config: Option<GcConfig>,
) {
    let config = config.unwrap_or_default();
    let gc = MarkBasedGarbageCollector::new(meta_store, object_client, config);

    gc.start().await;
}
