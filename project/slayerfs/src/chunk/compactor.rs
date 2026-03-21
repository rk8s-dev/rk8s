//! Compactor: coordinates MetaStore and BlockStore to compact chunk slices.
use crate::chuck::{
    ChunkLayout,
    slice::{SliceDesc, SliceOffset, block_span_iter_chunk, block_span_iter_slice},
    store::{BlockKey, BlockStore},
};
use crate::meta::SLICE_ID_KEY;
use crate::meta::config::CompactConfig;
use crate::meta::store::{MetaError, MetaStore};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactResult {
    Skipped,
    Light { removed: usize },
    Heavy { new_slice_id: u64 },
}

pub struct Compactor<B> {
    meta_store: Arc<dyn MetaStore>,
    block_store: Arc<B>,
    layout: ChunkLayout,
    config: CompactConfig,
}

impl<B> Compactor<B>
where
    B: BlockStore + Send + Sync + 'static,
{
    pub fn new(meta_store: Arc<dyn MetaStore>, block_store: Arc<B>) -> Self {
        Self {
            meta_store,
            block_store,
            layout: ChunkLayout::default(),
            config: CompactConfig::default(),
        }
    }

    pub fn with_layout(
        meta_store: Arc<dyn MetaStore>,
        block_store: Arc<B>,
        layout: ChunkLayout,
    ) -> Self {
        Self {
            meta_store,
            block_store,
            layout,
            config: CompactConfig::default(),
        }
    }

    pub fn with_config(
        meta_store: Arc<dyn MetaStore>,
        block_store: Arc<B>,
        layout: ChunkLayout,
        config: CompactConfig,
    ) -> Self {
        Self {
            meta_store,
            block_store,
            layout,
            config,
        }
    }

    pub fn block_store(&self) -> &Arc<B> {
        &self.block_store
    }

    pub fn meta_store(&self) -> &Arc<dyn MetaStore> {
        &self.meta_store
    }

    pub fn config(&self) -> &CompactConfig {
        &self.config
    }
    pub async fn analyze_chunk(&self, chunk_id: u64) -> Result<(usize, u64, f64), CompactorError> {
        let slices = self.meta_store.get_slices(chunk_id).await?;
        let count = slices.len();
        if count == 0 {
            return Ok((0, 0, 0.0));
        }
        let total: u64 = slices.iter().map(|s| s.length).sum();
        let frag = SliceDesc::calculate_fragmentation(&slices);
        Ok((count, total, frag))
    }

    /// Check whether a chunk should be compacted and, if so, whether it
    /// should be done synchronously (blocking writes).
    pub async fn should_compact(&self, chunk_id: u64) -> Result<(bool, bool), CompactorError> {
        let (count, _total, frag) = self.analyze_chunk(chunk_id).await?;
        let cfg = &self.config;

        if count < cfg.min_slice_count {
            return Ok((false, false));
        }
        if frag < cfg.min_fragment_ratio {
            return Ok((false, false));
        }
        let is_sync = count >= cfg.sync_threshold;
        Ok((true, is_sync))
    }

    /// Try to compact a chunk.  Runs light first; if fragmentation is still
    /// above threshold after light, falls through to heavy.
    pub async fn compact_chunk(&self, chunk_id: u64) -> Result<CompactResult, CompactorError> {
        let slices = self.meta_store.get_slices(chunk_id).await?;

        if slices.len() <= 1 {
            return Ok(CompactResult::Skipped);
        }

        let frag = SliceDesc::calculate_fragmentation(&slices);
        if frag < self.config.min_fragment_ratio {
            return Ok(CompactResult::Skipped);
        }

        let light_removed = match self.compact_light_inner(&slices, chunk_id).await? {
            Some(removed) if removed > 0 => removed,
            _ => 0,
        };

        let slices_after_light = self.meta_store.get_slices(chunk_id).await?;
        if slices_after_light.len() <= 1 {
            return if light_removed > 0 {
                Ok(CompactResult::Light {
                    removed: light_removed,
                })
            } else {
                Ok(CompactResult::Skipped)
            };
        }

        let frag_after_light = SliceDesc::calculate_fragmentation(&slices_after_light);
        if frag_after_light < self.config.min_fragment_ratio {
            return if light_removed > 0 {
                Ok(CompactResult::Light {
                    removed: light_removed,
                })
            } else {
                Ok(CompactResult::Skipped)
            };
        }

        let new_slice_id = self
            .compact_heavy_inner(&slices_after_light, chunk_id)
            .await?;
        Ok(CompactResult::Heavy { new_slice_id })
    }

    /// Scan all chunks and compact those exceeding the configured thresholds.
    /// Returns the number of chunks actually compacted.
    pub async fn run_compaction_cycle(&self) -> Result<usize, CompactorError> {
        let chunk_ids = self
            .meta_store
            .list_chunk_ids(self.config.max_chunks_per_run)
            .await?;

        let mut compacted = 0usize;
        for chunk_id in chunk_ids {
            match self.should_compact(chunk_id).await {
                Ok((true, _is_sync)) => match self.compact_chunk(chunk_id).await {
                    Ok(CompactResult::Skipped) => {}
                    Ok(_result) => {
                        compacted += 1;
                    }
                    Err(e) => {
                        warn!(chunk_id, error = %e, "failed to compact chunk");
                    }
                },
                Ok((false, _)) => {}
                Err(e) => {
                    warn!(chunk_id, error = %e, "error checking compaction status");
                }
            }
        }
        Ok(compacted)
    }

    pub async fn compact_light(&self, chunk_id: u64) -> Result<Option<usize>, CompactorError> {
        let slices = self.meta_store.get_slices(chunk_id).await?;
        self.compact_light_inner(&slices, chunk_id).await
    }

    async fn compact_light_inner(
        &self,
        slices: &[SliceDesc],
        chunk_id: u64,
    ) -> Result<Option<usize>, CompactorError> {
        if slices.len() <= 1 {
            return Ok(None);
        }

        let merged = SliceDesc::remove_fully_covered(slices);
        let replaced_ids = SliceDesc::find_replaced_ids(slices, &merged);

        if replaced_ids.is_empty() {
            return Ok(None);
        }

        let delayed = SliceDesc::encode_delayed_data(slices, &replaced_ids);

        // Atomic: delete old slice_meta + insert delayed records.
        // Uses replace_slices_for_compact with NO new slices
        self.meta_store
            .replace_slices_for_compact(chunk_id, &[], &delayed)
            .await?;

        let removed = replaced_ids.len();

        Ok(Some(removed))
    }

    /// Data-rewrite compaction: read all blocks, merge, write new slice.
    pub async fn compact_heavy(&self, chunk_id: u64) -> Result<u64, CompactorError> {
        let slices = self.meta_store.get_slices(chunk_id).await?;
        self.compact_heavy_inner(&slices, chunk_id).await
    }

    async fn compact_heavy_inner(
        &self,
        slices: &[SliceDesc],
        chunk_id: u64,
    ) -> Result<u64, CompactorError> {
        let chunk_size = self.layout.chunk_size;
        let mut merged_data = vec![0u8; chunk_size as usize];

        self.read_and_merge_slices(slices, &mut merged_data).await?;

        let new_slice_id = self.meta_store.next_id(SLICE_ID_KEY).await? as u64;

        self.meta_store
            .record_uncommitted_slice(new_slice_id, chunk_id, chunk_size, "compact_heavy")
            .await
            .map_err(CompactorError::MetaError)?;

        self.write_merged_data(new_slice_id, &merged_data).await?;

        let new_slice = SliceDesc {
            slice_id: new_slice_id,
            chunk_id,
            offset: 0,
            length: chunk_size,
        };

        let all_ids: Vec<u64> = slices.iter().map(|s| s.slice_id).collect();
        let delayed = SliceDesc::encode_delayed_data(slices, &all_ids);
        let expected_count = slices.len();

        match self
            .meta_store
            .replace_slices_for_compact_with_version(
                chunk_id,
                &[new_slice],
                &delayed,
                expected_count,
            )
            .await
        {
            Ok(()) => {
                if let Err(e) = self.meta_store.confirm_slice_committed(new_slice_id).await {
                    warn!(
                        chunk_id,
                        new_slice_id,
                        error = %e,
                        "Failed to confirm slice committed, will be cleaned up by GC"
                    );
                }
                Ok(new_slice_id)
            }
            Err(MetaError::ContinueRetry) => {
                warn!(
                    chunk_id,
                    new_slice_id, "Compact heavy conflict detected, retry needed"
                );
                if let Err(cleanup_err) = self
                    .cleanup_uncommitted_slice(new_slice_id, chunk_size)
                    .await
                {
                    warn!(
                        chunk_id,
                        new_slice_id,
                        error = %cleanup_err,
                        "Failed to cleanup uncommitted slice after conflict"
                    );
                }
                Err(CompactorError::MetaError(MetaError::ContinueRetry))
            }
            Err(e) => {
                if let Err(cleanup_err) = self
                    .cleanup_uncommitted_slice(new_slice_id, chunk_size)
                    .await
                {
                    warn!(
                        chunk_id,
                        new_slice_id,
                        error = %cleanup_err,
                        "Failed to cleanup uncommitted slice after error"
                    );
                }
                Err(CompactorError::MetaError(e))
            }
        }
    }

    /// Read all slices and merge; newer slices (higher slice_id) overwrite older ones.
    async fn read_and_merge_slices(
        &self,
        slices: &[SliceDesc],
        merged_data: &mut [u8],
    ) -> Result<(), CompactorError> {
        let mut sorted: Vec<_> = slices.to_vec();
        sorted.sort_by_key(|s| s.slice_id);

        for slice in sorted {
            let data = self.read_slice_data(&slice).await?;
            let start = slice.offset as usize;
            let end = start + slice.length as usize;

            if end > merged_data.len() {
                return Err(CompactorError::InvalidData(format!(
                    "Slice {} exceeds chunk bounds: offset={}, length={}, chunk_size={}",
                    slice.slice_id,
                    slice.offset,
                    slice.length,
                    merged_data.len()
                )));
            }

            merged_data[start..end].copy_from_slice(&data);
        }

        Ok(())
    }

    async fn read_slice_data(&self, slice: &SliceDesc) -> Result<Vec<u8>, CompactorError> {
        let mut data = vec![0u8; slice.length as usize];
        let spans: Vec<_> =
            block_span_iter_slice(SliceOffset(0), slice.length, self.layout).collect();

        let mut pos = 0usize;
        for span in spans {
            let key: BlockKey = (slice.slice_id, span.index as u32);
            let take = span.len as usize;
            self.block_store
                .read_range(key, span.offset, &mut data[pos..pos + take])
                .await
                .map_err(|e| CompactorError::BlockStoreError(e.to_string()))?;
            pos += take;
        }

        Ok(data)
    }

    async fn write_merged_data(&self, slice_id: u64, data: &[u8]) -> Result<(), CompactorError> {
        let spans: Vec<_> =
            block_span_iter_chunk(0u64.into(), data.len() as u64, self.layout).collect();

        let mut offset = 0usize;
        for span in spans {
            let key: BlockKey = (slice_id, span.index as u32);
            let take = (span.len as usize).min(data.len() - offset);
            self.block_store
                .write_fresh_range(key, span.offset, &data[offset..offset + take])
                .await
                .map_err(|e| CompactorError::BlockStoreError(e.to_string()))?;
            offset += take;
        }

        Ok(())
    }

    /// Clean up uncommitted slice data when compaction fails.
    /// This prevents orphan block data from accumulating.
    async fn cleanup_uncommitted_slice(
        &self,
        slice_id: u64,
        size: u64,
    ) -> Result<(), CompactorError> {
        // Delete block data from block store
        let num_blocks = size.div_ceil(self.layout.block_size as u64);
        if num_blocks > 0 {
            self.block_store
                .delete_range((slice_id, 0), num_blocks)
                .await
                .map_err(|e| {
                    CompactorError::BlockStoreError(format!(
                        "Failed to delete uncommitted blocks for slice {}: {}",
                        slice_id, e
                    ))
                })?;
        }

        // Note: The uncommitted_slice record in metadata will be cleaned up by
        // cleanup_orphan_uncommitted_slices during GC if it wasn't confirmed.
        // We don't delete it here because:
        // 1. It helps with crash recovery tracking
        // 2. GC will handle it based on age
        // 3. Avoid race conditions with concurrent operations

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CompactorError {
    #[error("MetaStore error: {0}")]
    MetaError(#[from] MetaError),
    #[error("BlockStore error: {0}")]
    BlockStoreError(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl From<anyhow::Error> for CompactorError {
    fn from(e: anyhow::Error) -> Self {
        CompactorError::BlockStoreError(e.to_string())
    }
}
