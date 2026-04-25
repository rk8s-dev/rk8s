//! Compactor: coordinates MetaStore and BlockStore to compact chunk slices.
use crate::chunk::ChunkLayout;
use crate::chunk::slice::{SliceDesc, SliceOffset, block_span_iter_chunk, block_span_iter_slice};
use crate::chunk::store::{BlockKey, BlockStore};
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

pub struct Compactor<B, M: MetaStore + ?Sized> {
    meta_store: Arc<M>,
    block_store: Arc<B>,
    layout: ChunkLayout,
    config: CompactConfig,
}

impl<B, M: ?Sized> Compactor<B, M>
where
    M: MetaStore + Send + Sync + 'static,
    B: BlockStore + Send + Sync + 'static,
{
    #[allow(dead_code)]
    pub fn new(meta_store: Arc<M>, block_store: Arc<B>) -> Self {
        Self {
            meta_store,
            block_store,
            layout: ChunkLayout::default(),
            config: CompactConfig::default(),
        }
    }

    #[allow(dead_code)]
    pub fn with_layout(meta_store: Arc<M>, block_store: Arc<B>, layout: ChunkLayout) -> Self {
        Self {
            meta_store,
            block_store,
            layout,
            config: CompactConfig::default(),
        }
    }

    #[allow(dead_code)]
    pub fn with_config(
        meta_store: Arc<M>,
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

    /// Try to compact a chunk with sequential light-then-heavy strategy.
    pub async fn compact_sequential(&self, chunk_id: u64) -> Result<CompactResult, CompactorError> {
        let (count, _, frag) = self.analyze_chunk(chunk_id).await?;
        if count <= 1 {
            return Ok(CompactResult::Skipped);
        }

        // Determine if heavy compaction might be needed
        let mut needs_heavy = self.config.heavy_enabled
            && (frag >= self.config.heavy_fragment_threshold
                || count >= self.config.heavy_slice_threshold);

        let mut light_removed = 0usize;
        if self.config.light_enabled && count >= self.config.light_threshold {
            let removed = self.compact_light(chunk_id).await?;

            if let Some(n) = removed {
                light_removed = n;

                // Re-analyze
                if n > 0 {
                    let (count_after, _, frag_after) = self.analyze_chunk(chunk_id).await?;
                    needs_heavy = self.config.heavy_enabled
                        && (frag_after >= self.config.heavy_fragment_threshold
                            || count_after >= self.config.heavy_slice_threshold);
                    // if remaining slices are extremely fragmented, force heavy
                    if frag_after >= self.config.heavy_force_fragment_threshold {
                        needs_heavy = true;
                    }
                    // If light removed all redundancy, skip heavy
                    if count_after <= 1 {
                        needs_heavy = false;
                    }
                }
            }
        }
        if needs_heavy {
            let new_slice_id = self.compact_heavy(chunk_id).await?;
            return Ok(CompactResult::Heavy { new_slice_id });
        }
        if light_removed > 0 {
            return Ok(CompactResult::Light {
                removed: light_removed,
            });
        }

        Ok(CompactResult::Skipped)
    }

    #[allow(dead_code)]
    pub async fn compact_chunk(&self, chunk_id: u64) -> Result<CompactResult, CompactorError> {
        self.compact_sequential(chunk_id).await
    }
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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

        match self
            .meta_store
            .replace_slices_for_compact_with_version(chunk_id, &[new_slice], &delayed, slices)
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

            self.read_slice_data_into(&slice, &mut merged_data[start..end])
                .await?;
        }

        Ok(())
    }

    async fn read_slice_data_into(
        &self,
        slice: &SliceDesc,
        dest: &mut [u8],
    ) -> Result<(), CompactorError> {
        let spans: Vec<_> =
            block_span_iter_slice(SliceOffset(0), slice.length, self.layout).collect();

        let mut pos = 0usize;
        for span in spans {
            let key: BlockKey = (slice.slice_id, span.index as u32);
            let take = span.len as usize;
            self.block_store
                .read_range(key, span.offset, &mut dest[pos..pos + take])
                .await
                .map_err(|e| CompactorError::BlockStoreError(e.to_string()))?;
            pos += take;
        }

        Ok(())
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
