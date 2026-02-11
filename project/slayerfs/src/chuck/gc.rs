//! Object Storage Garbage Collector
//!
//! This module provides garbage collection for object storage backends.

use crate::chuck::store::BlockStore;
use crate::meta::store::{MetaStore, MetaError};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Configuration for the garbage collector.
#[derive(Debug, Clone)]
pub struct GCConfig {
    /// Interval between GC runs
    pub interval: Duration,
    /// Minimum age of delayed slices before deletion (safety period)
    pub min_age_secs: i64,
    /// Maximum number of slices to process per run
    pub batch_size: usize,
}

impl Default for GCConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(3600), // 1 hour
            min_age_secs: 3600,                   // 1 hour safety period
            batch_size: 1000,
        }
    }
}

/// Garbage collector for object storage block data.
pub struct BlockStoreGC<M, B> {
    meta_store: Arc<M>,
    block_store: Arc<B>,
}

impl<M, B> BlockStoreGC<M, B>
where
    M: MetaStore + Send + Sync + 'static,
    B: BlockStore + Send + Sync + 'static,
{
    /// Create a new garbage collector.
    pub fn new(meta_store: Arc<M>, block_store: Arc<B>) -> Self {
        Self {
            meta_store,
            block_store,
        }
    }

    /// Start the garbage collector background task.
    pub fn start(self, config: GCConfig) -> tokio::task::JoinHandle<()> {
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

    /// Run a single GC cycle.
    pub async fn run_gc_cycle(&self, config: &GCConfig) -> Result<(), GCError> {
        // Process delayed slices from metadata store
        let deleted_count = self
            .meta_store
            .process_delayed_slices(config.batch_size, config.min_age_secs)
            .await
            .map_err(|e| GCError::MetaError(e))?;

        if deleted_count > 0 {
            info!(
                deleted_count = deleted_count,
                "GC cycle completed, processed delayed slices"
            );
        } else {
            debug!("GC cycle completed, no slices to process");
        }

        Ok(())
    }
}

/// Errors that can occur during GC.
#[derive(Debug)]
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
