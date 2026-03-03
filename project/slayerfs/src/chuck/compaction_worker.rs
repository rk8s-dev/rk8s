//! Compaction Worker
//! Background task for automatic chunk compaction.
//! This module provides a background worker that periodically scans chunks
//! and compacts those that meet the configured thresholds.

use crate::chuck::{BlockGcConfig, BlockStoreGC, Compactor};
use crate::meta::store::MetaStore;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Configuration for the compaction worker.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CompactionWorkerConfig {
    /// interval between compaction scans
    pub scan_interval: Duration,
    /// maximum number of chunks to compact per run
    pub max_chunks_per_run: usize,
    /// enable automatic compaction
    pub enabled: bool,
}

impl Default for CompactionWorkerConfig {
    fn default() -> Self {
        Self {
            scan_interval: Duration::from_secs(3600), // s
            max_chunks_per_run: 100,
            enabled: true,
        }
    }
}

#[allow(dead_code)]
pub struct CompactionWorker<M, B> {
    meta_store: Arc<M>,
    compactor: Arc<Compactor<M, B>>,
}

impl<M, B> CompactionWorker<M, B>
where
    M: MetaStore + Send + Sync + 'static,
    B: crate::chuck::BlockStore + Send + Sync + 'static,
{
    #[allow(dead_code)]
    pub fn new(meta_store: Arc<M>, block_store: Arc<B>) -> Self {
        let compactor = Arc::new(Compactor::new(meta_store.clone(), block_store));
        Self {
            meta_store,
            compactor,
        }
    }

    /// start the compaction worker and GC as background tasks, returns the join handles for both tasks.
    #[allow(dead_code)]
    pub fn start(
        self,
        worker_config: CompactionWorkerConfig,
        gc_config: BlockGcConfig,
    ) -> (tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>) {
        let compactor = self.compactor.clone();
        let meta_store = self.meta_store.clone();
        let compaction_handle = tokio::spawn(async move {
            if !worker_config.enabled {
                return;
            }

            let mut ticker = interval(worker_config.scan_interval);
            loop {
                ticker.tick().await;

                if let Err(e) = run_compaction_cycle(&meta_store, &compactor, &worker_config).await
                {
                    error!(error = %e, "Compaction cycle failed");
                }
            }
        });
        let gc_handle = BlockStoreGC::new(self.meta_store, self.compactor.block_store().clone())
            .start(gc_config);

        (compaction_handle, gc_handle)
    }
}

/// run a single compaction cycle.
#[allow(dead_code)]
async fn run_compaction_cycle<M, B>(
    meta_store: &Arc<M>,
    _compactor: &Arc<Compactor<M, B>>,
    _config: &CompactionWorkerConfig,
) -> anyhow::Result<()>
where
    M: MetaStore + Send + Sync,
    B: crate::chuck::BlockStore + Send + Sync,
{
    // debug!("starting compaction cycle");
    let compacted = meta_store
        .run_compact_by_threshold()
        .await
        .map_err(|e| anyhow::anyhow!("MetaStore compaction failed: {}", e))?;

    if compacted > 0 {
        info!(compacted_chunks = compacted, "compaction cycle completed");
    } else {
        debug!("compaction cycle completed, no chunks needed compaction");
    }

    Ok(())
}
