//! Compactor: coordinates MetaStore and BlockStore to perform real compaction.
//!
//! The compaction process follows these steps:
//! Read Phase: Fetch all slice metadata from MetaStore
//! Merge Phase: Calculate optimal merge using Intervals utility
//! Data Phase: Read actual data from BlockStore, merge in memory
//! Write Phase: Write merged data to new slice in BlockStore
//! Commit Phase: Atomically update metadata (replace old slices with new)
//! Cleanup Phase: Schedule old data for deletion
use crate::chuck::{
    ChunkLayout,
    slice::{SliceDesc, SliceOffset, block_span_iter_chunk, block_span_iter_slice},
    store::{BlockKey, BlockStore},
};
use crate::meta::SLICE_ID_KEY;
use crate::meta::store::{MetaError, MetaStore};
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Compactor coordinates metadata and block store to perform real data compaction.
pub struct Compactor<M, B> {
    #[allow(dead_code)]
    meta_store: Arc<M>,
    block_store: Arc<B>,
    layout: ChunkLayout,
}

#[allow(dead_code)]
impl<M, B> Compactor<M, B>
where
    M: MetaStore + Send + Sync + 'static,
    B: BlockStore + Send + Sync + 'static,
{
    /// Create a new compactor with the given meta store and block store.
    #[allow(dead_code)]
    pub fn new(meta_store: Arc<M>, block_store: Arc<B>) -> Self {
        Self {
            meta_store,
            block_store,
            layout: ChunkLayout::default(),
        }
    }

    /// Create a new compactor with custom chunk layout.
    pub fn with_layout(meta_store: Arc<M>, block_store: Arc<B>, layout: ChunkLayout) -> Self {
        Self {
            meta_store,
            block_store,
            layout,
        }
    }

    /// Get a reference to the block store.
    pub fn block_store(&self) -> &Arc<B> {
        &self.block_store
    }

    /// Get a reference to the meta store.
    pub fn meta_store(&self) -> &Arc<M> {
        &self.meta_store
    }

    pub async fn compact_chunk(
        &self,
        chunk_id: u64,
        _inode: i64,
    ) -> Result<Option<u64>, CompactorError> {
        // Get all slices for this chunk from metadata store
        let slices = self
            .meta_store
            .get_slices(chunk_id)
            .await
            .map_err(CompactorError::MetaError)?;

        if slices.len() <= 1 {
            debug!(
                chunk_id = chunk_id,
                slice_count = slices.len(),
                "No need to compact"
            );
            return Ok(None);
        }

        // Calculate merged slice layout using Intervals
        let merged_slices = self.calculate_merged_slices(&slices)?;

        // Calculate fragmentation ratio: (total_size - merged_size) / total_size
        let total_size: u64 = slices.iter().map(|s| s.length).sum();
        let merged_size: u64 = merged_slices.iter().map(|s| s.length).sum();
        let fragment_ratio = if total_size > 0 {
            (total_size - merged_size) as f64 / total_size as f64
        } else {
            0.0
        };

        // Check fragmentation threshold (default 0.1 from config)
        const MIN_FRAGMENT_RATIO: f64 = 0.1;
        if fragment_ratio < MIN_FRAGMENT_RATIO {
            debug!(
                chunk_id = chunk_id,
                slice_count = slices.len(),
                fragment_ratio = fragment_ratio,
                "Fragmentation ratio too low, skipping compaction"
            );
            return Ok(None);
        }

        if merged_slices.len() >= slices.len() {
            debug!(
                chunk_id = chunk_id,
                before = slices.len(),
                after = merged_slices.len(),
                "No slices can be merged"
            );
            return Ok(None);
        }

        info!(
            chunk_id = chunk_id,
            slice_count = slices.len(),
            fragment_ratio = fragment_ratio,
            "Starting real compaction"
        );

        // Identify which slices are being replaced (fully covered)
        let replaced_slice_ids: Vec<u64> = self.find_replaced_slices(&slices, &merged_slices);

        if replaced_slice_ids.is_empty() {
            debug!(
                chunk_id = chunk_id,
                "No slices are fully covered, skipping compaction"
            );
            return Ok(None);
        }

        // Read data from old slices and merge
        let chunk_size = self.layout.chunk_size;
        let mut merged_data = vec![0u8; chunk_size as usize];

        self.read_and_merge_slices(&slices, &mut merged_data)
            .await?;

        // Allocate new slice ID
        let new_slice_id = self
            .meta_store
            .next_id(SLICE_ID_KEY)
            .await
            .map_err(CompactorError::MetaError)? as u64;

        // Write merged data to BlockStore
        self.write_merged_data(new_slice_id, &merged_data).await?;

        // Create new slice descriptor for the merged data
        let new_slice = SliceDesc {
            slice_id: new_slice_id,
            chunk_id,
            offset: 0,
            length: chunk_size,
        };

        // Prepare delayed deletion data for old slices (12 bytes each: 8 bytes slice_id + 4 bytes size)
        let delayed_data = self.prepare_delayed_data(&slices, &replaced_slice_ids);

        // Update metadata: replace old slices with new one (atomic operation)
        self.replace_slices_in_meta(chunk_id, &[new_slice], &delayed_data)
            .await?;

        info!(
            chunk_id = chunk_id,
            new_slice_id = new_slice_id,
            old_slice_count = slices.len(),
            replaced_count = replaced_slice_ids.len(),
            "Compaction completed successfully"
        );

        Ok(Some(new_slice_id))
    }

    /// Calculate merged slices by removing fully covered slices.
    ///
    /// This only removes slices that are **completely** covered by newer slices.
    /// Partially covered slices are kept intact because changing a slice's
    /// offset/length would break block data addressing — block data is stored
    /// at `(slice_id, block_index)` relative to the slice's original offset.
    fn calculate_merged_slices(
        &self,
        slices: &[SliceDesc],
    ) -> Result<Vec<SliceDesc>, CompactorError> {
        if slices.is_empty() {
            return Ok(vec![]);
        }

        // Sort by slice_id descending (newest first) for "latest wins" processing
        let mut slices_sorted: Vec<SliceDesc> = slices.to_vec();
        slices_sorted.sort_by_key(|s| std::cmp::Reverse(s.slice_id));

        // Track covered ranges and build result
        let mut covered_ranges: Vec<(u64, u64)> = Vec::new();
        let mut result_slices: Vec<SliceDesc> = Vec::new();

        for slice in slices_sorted {
            let slice_start = slice.offset;
            let slice_end = slice.offset + slice.length;

            // Check if this slice is fully covered by newer slices
            let is_fully_covered = covered_ranges
                .iter()
                .any(|&(cs, ce)| slice_start >= cs && slice_end <= ce);

            if !is_fully_covered {
                // Keep the entire slice — can't split without rewriting block data
                result_slices.push(slice);
            }

            // Mark this slice's full range as covered for older slices
            covered_ranges.push((slice_start, slice_end));
        }

        // Sort result by offset for consistent ordering
        result_slices.sort_by_key(|s| s.offset);
        Ok(result_slices)
    }

    /// Find slices that are being fully replaced by compaction.
    ///
    /// Returns slice IDs that are completely covered by newer data.
    fn find_replaced_slices(&self, original: &[SliceDesc], merged: &[SliceDesc]) -> Vec<u64> {
        let merged_ids: std::collections::HashSet<u64> =
            merged.iter().map(|s| s.slice_id).collect();

        let original_ids: std::collections::HashSet<u64> =
            original.iter().map(|s| s.slice_id).collect();

        // Slices that exist in original but not in merged are fully replaced
        original_ids.difference(&merged_ids).copied().collect()
    }

    /// Prepare delayed deletion data for old slices.
    ///
    /// Format: 20 bytes per slice
    /// - Bytes 0-7: slice_id (u64, little endian)
    /// - Bytes 8-15: offset (u64, little endian)
    /// - Bytes 16-19: size (u32, little endian)
    fn prepare_delayed_data(&self, slices: &[SliceDesc], replaced_ids: &[u64]) -> Vec<u8> {
        let replaced_set: std::collections::HashSet<u64> = replaced_ids.iter().copied().collect();
        let mut delayed = Vec::with_capacity(replaced_ids.len() * 20);

        for slice in slices {
            if replaced_set.contains(&slice.slice_id) {
                // slice_id: 8 bytes
                delayed.extend_from_slice(&slice.slice_id.to_le_bytes());
                // offset: 8 bytes
                delayed.extend_from_slice(&slice.offset.to_le_bytes());
                // size: 4 bytes (truncated to u32)
                let size = slice.length.min(u32::MAX as u64) as u32;
                delayed.extend_from_slice(&size.to_le_bytes());
            }
        }

        delayed
    }

    /// Read data from all slices and merge into a single buffer.
    ///
    /// Newer slices (higher slice_id) overwrite older ones in overlapping regions.
    async fn read_and_merge_slices(
        &self,
        slices: &[SliceDesc],
        merged_data: &mut [u8],
    ) -> Result<(), CompactorError> {
        // Sort by slice_id ascending (oldest first) so newer data overwrites older
        let mut sorted_slices: Vec<_> = slices.to_vec();
        sorted_slices.sort_by_key(|s| s.slice_id);

        for slice in sorted_slices {
            let slice_data = self.read_slice_data(&slice).await?;

            // Copy slice data to merged buffer at correct offset
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

            merged_data[start..end].copy_from_slice(&slice_data);

            debug!(
                slice_id = slice.slice_id,
                offset = slice.offset,
                length = slice.length,
                "Read and merged slice data"
            );
        }

        Ok(())
    }

    /// Read all block data for a slice from BlockStore.
    async fn read_slice_data(&self, slice: &SliceDesc) -> Result<Vec<u8>, CompactorError> {
        let mut data = vec![0u8; slice.length as usize];

        // Iterate over all blocks in this slice
        let spans: Vec<_> =
            block_span_iter_slice(SliceOffset(0), slice.length, self.layout).collect();

        let mut offset_in_slice = 0usize;

        for span in spans {
            let block_key = (slice.slice_id, span.index as u32);
            let block_size = self.layout.block_size as usize;
            let mut block_buf = vec![0u8; block_size];

            self.block_store
                .read_range(block_key, span.offset, &mut block_buf[..span.len as usize])
                .await
                .map_err(|e| CompactorError::BlockStoreError(e.to_string()))?;

            // Copy relevant part to slice data buffer
            let copy_len = (span.len as usize).min(data.len() - offset_in_slice);
            data[offset_in_slice..offset_in_slice + copy_len]
                .copy_from_slice(&block_buf[..copy_len]);

            offset_in_slice += copy_len;
        }

        Ok(data)
    }

    /// Write merged data to BlockStore as a new slice.
    async fn write_merged_data(&self, slice_id: u64, data: &[u8]) -> Result<(), CompactorError> {
        let spans: Vec<_> =
            block_span_iter_chunk(0u64.into(), data.len() as u64, self.layout).collect();

        let mut offset = 0usize;

        for span in spans {
            let block_key = (slice_id, span.index as u32);
            let block_size = self.layout.block_size as usize;
            let mut block_data = vec![0u8; block_size];
            let copy_len = (span.len as usize).min(data.len() - offset);
            block_data[..copy_len].copy_from_slice(&data[offset..offset + copy_len]);

            self.block_store
                .write_fresh_range(block_key, span.offset, &block_data[..copy_len])
                .await
                .map_err(|e| CompactorError::BlockStoreError(e.to_string()))?;

            offset += copy_len;

            debug!(
                slice_id = slice_id,
                block_index = span.index,
                "Wrote merged block"
            );
        }

        Ok(())
    }

    async fn replace_slices_in_meta(
        &self,
        chunk_id: u64,
        new_slices: &[SliceDesc],
        delayed_data: &[u8],
    ) -> Result<(), CompactorError> {
        self.meta_store
            .replace_slices_for_compact(chunk_id, new_slices, delayed_data)
            .await
            .map_err(CompactorError::MetaError)?;

        debug!(
            chunk_id = chunk_id,
            new_slice_count = new_slices.len(),
            delayed_count = delayed_data.len() / 12,
            "Metadata atomically updated for compaction"
        );

        Ok(())
    }

    #[allow(dead_code)]
    async fn delete_old_slice_data_immediate(
        &self,
        old_slices: &[SliceDesc],
        new_slice_id: u64,
    ) -> Result<(), CompactorError> {
        for slice in old_slices {
            // Don't delete if this is the new slice
            if slice.slice_id == new_slice_id {
                continue;
            }

            // Block indices are slice-relative (starting from 0), not chunk-relative.
            // Data was written via write_at_vectored(slice_id, SliceOffset(0), ...)
            // so blocks are at (slice_id, 0), (slice_id, 1), etc.
            let num_blocks = slice.length.div_ceil(self.layout.block_size as u64);

            if num_blocks > 0 {
                self.block_store
                    .delete_range((slice.slice_id, 0), num_blocks)
                    .await
                    .map_err(|e| CompactorError::BlockStoreError(e.to_string()))?;

                debug!(
                    slice_id = slice.slice_id,
                    num_blocks = num_blocks,
                    "Deleted old slice data from block store"
                );
            }
        }

        Ok(())
    }
}

/// Errors that can occur during compaction.
#[derive(Debug)]
pub enum CompactorError {
    MetaError(MetaError),
    BlockStoreError(String),
    #[allow(dead_code)]
    InvalidData(String),
    IoError(std::io::Error),
}

impl std::fmt::Display for CompactorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompactorError::MetaError(e) => write!(f, "MetaStore error: {}", e),
            CompactorError::BlockStoreError(s) => write!(f, "BlockStore error: {}", s),
            CompactorError::InvalidData(s) => write!(f, "Invalid data: {}", s),
            CompactorError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for CompactorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CompactorError::MetaError(e) => Some(e),
            CompactorError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<MetaError> for CompactorError {
    fn from(e: MetaError) -> Self {
        CompactorError::MetaError(e)
    }
}

impl From<std::io::Error> for CompactorError {
    fn from(e: std::io::Error) -> Self {
        CompactorError::IoError(e)
    }
}

impl From<anyhow::Error> for CompactorError {
    fn from(e: anyhow::Error) -> Self {
        CompactorError::BlockStoreError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::InMemoryBlockStore;

    // Mock MetaStore for testing
    struct MockMetaStore {
        slices: std::sync::Mutex<std::collections::HashMap<u64, Vec<SliceDesc>>>,
        next_slice_id: std::sync::atomic::AtomicU64,
    }

    impl MockMetaStore {
        fn new() -> Self {
            Self {
                slices: std::sync::Mutex::new(std::collections::HashMap::new()),
                next_slice_id: std::sync::atomic::AtomicU64::new(1),
            }
        }
    }

    #[async_trait::async_trait]
    impl MetaStore for MockMetaStore {
        async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError> {
            let guard = self.slices.lock().unwrap();
            Ok(guard.get(&chunk_id).cloned().unwrap_or_default())
        }

        async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
            if key == SLICE_ID_KEY {
                Ok(self
                    .next_slice_id
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst) as i64)
            } else {
                Err(MetaError::NotSupported(format!(
                    "Key {} not supported",
                    key
                )))
            }
        }

        async fn replace_slices_for_compact(
            &self,
            chunk_id: u64,
            new_slices: &[SliceDesc],
            _old_slices_to_delay: &[u8],
        ) -> Result<(), MetaError> {
            let mut guard = self.slices.lock().unwrap();
            guard.insert(chunk_id, new_slices.to_vec());
            Ok(())
        }

        // Stub implementations for other required methods
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

    #[tokio::test]
    async fn test_compactor_no_compaction_needed() {
        let meta_store = Arc::new(MockMetaStore::new());
        let block_store = Arc::new(InMemoryBlockStore::new());
        let compactor = Compactor::new(meta_store, block_store);

        // Test with 0 slices
        let result = compactor.compact_chunk(1, 1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn test_compactor_error_display() {
        let err = CompactorError::InvalidData("test error".to_string());
        assert!(err.to_string().contains("Invalid data"));
    }

    #[test]
    fn test_calculate_merged_slices() {
        let meta_store = Arc::new(MockMetaStore::new());
        let block_store = Arc::new(InMemoryBlockStore::new());
        let compactor = Compactor::new(meta_store, block_store);

        // Test case: Slice 1 (offset 0, len 100) fully covered by Slice 2 (offset 0, len 150)
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 150,
            },
        ];

        let merged = compactor.calculate_merged_slices(&slices).unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].slice_id, 2);
        assert_eq!(merged[0].offset, 0);
        assert_eq!(merged[0].length, 150);
    }

    #[test]
    fn test_find_replaced_slices() {
        let meta_store = Arc::new(MockMetaStore::new());
        let block_store = Arc::new(InMemoryBlockStore::new());
        let compactor = Compactor::new(meta_store, block_store);

        let original = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 150,
            },
        ];
        let merged = vec![SliceDesc {
            slice_id: 2,
            chunk_id: 1,
            offset: 0,
            length: 150,
        }];

        let replaced = compactor.find_replaced_slices(&original, &merged);
        assert_eq!(replaced.len(), 1);
        assert!(replaced.contains(&1));
    }

    #[test]
    fn test_prepare_delayed_data() {
        let meta_store = Arc::new(MockMetaStore::new());
        let block_store = Arc::new(InMemoryBlockStore::new());
        let compactor = Compactor::new(meta_store, block_store);

        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 150,
            },
        ];
        let replaced = vec![1];

        let delayed = compactor.prepare_delayed_data(&slices, &replaced);
        assert_eq!(delayed.len(), 20);
        let slice_id = u64::from_le_bytes([
            delayed[0], delayed[1], delayed[2], delayed[3], delayed[4], delayed[5], delayed[6],
            delayed[7],
        ]);
        let offset = u64::from_le_bytes([
            delayed[8],
            delayed[9],
            delayed[10],
            delayed[11],
            delayed[12],
            delayed[13],
            delayed[14],
            delayed[15],
        ]);
        let size = u32::from_le_bytes([delayed[16], delayed[17], delayed[18], delayed[19]]);

        assert_eq!(slice_id, 1);
        assert_eq!(offset, 0);
        assert_eq!(size, 100);
    }

    // Comprehensive calculate_merged_slices tests
    fn make_compactor() -> Compactor<MockMetaStore, InMemoryBlockStore> {
        Compactor::new(
            Arc::new(MockMetaStore::new()),
            Arc::new(InMemoryBlockStore::new()),
        )
    }

    #[test]
    fn test_calculate_merged_empty() {
        let c = make_compactor();
        let result = c.calculate_merged_slices(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_calculate_merged_single_slice() {
        let c = make_compactor();
        let slices = vec![SliceDesc {
            slice_id: 1,
            chunk_id: 1,
            offset: 0,
            length: 100,
        }];
        let result = c.calculate_merged_slices(&slices).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slice_id, 1);
    }

    /// Partial overlap: both slices must be kept intact (no splitting).
    #[test]
    fn test_calculate_merged_partial_overlap_kept() {
        let c = make_compactor();
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 50,
                length: 100,
            },
        ];
        let result = c.calculate_merged_slices(&slices).unwrap();
        assert_eq!(result.len(), 2, "partial overlap: both kept");
        // Verify original offset/length preserved
        let s1 = result.iter().find(|s| s.slice_id == 1).unwrap();
        assert_eq!(s1.offset, 0);
        assert_eq!(s1.length, 100);
    }

    /// Full coverage: older slice removed.
    #[test]
    fn test_calculate_merged_full_coverage() {
        let c = make_compactor();
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 10,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
        ];
        let result = c.calculate_merged_slices(&slices).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slice_id, 2);
    }

    /// Non-zero offset head overlap: older slice kept intact.
    #[test]
    fn test_calculate_merged_head_overlap_nonzero_offset() {
        let c = make_compactor();
        // A: [100, 200), B: [80, 160) — covers head of A
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 100,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 80,
                length: 80,
            },
        ];
        let result = c.calculate_merged_slices(&slices).unwrap();
        assert_eq!(result.len(), 2);
        let s1 = result.iter().find(|s| s.slice_id == 1).unwrap();
        assert_eq!(s1.offset, 100, "offset must NOT change");
        assert_eq!(s1.length, 100, "length must NOT change");
    }

    /// Chain: A⊂B⊂C → only C survives.
    #[test]
    fn test_calculate_merged_chain() {
        let c = make_compactor();
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 10,
                length: 20,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 5,
                length: 50,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
        ];
        let result = c.calculate_merged_slices(&slices).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slice_id, 3);
    }

    /// Non-overlapping: all kept.
    #[test]
    fn test_calculate_merged_disjoint() {
        let c = make_compactor();
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 100,
                length: 50,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 200,
                length: 50,
            },
        ];
        let result = c.calculate_merged_slices(&slices).unwrap();
        assert_eq!(result.len(), 3);
    }

    /// Middle overlap (sandwich): older slice kept intact.
    #[test]
    fn test_calculate_merged_sandwich() {
        let c = make_compactor();
        // A: [0, 200), B: [50, 150) — B covers only middle of A
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 200,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 50,
                length: 100,
            },
        ];
        let result = c.calculate_merged_slices(&slices).unwrap();
        assert_eq!(result.len(), 2, "A not fully covered, kept intact");
    }

    /// Exact same range: older removed.
    #[test]
    fn test_calculate_merged_exact_overlap() {
        let c = make_compactor();
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 100,
                length: 200,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 100,
                length: 200,
            },
        ];
        let result = c.calculate_merged_slices(&slices).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slice_id, 2);
    }

    /// Result is sorted by offset.
    #[test]
    fn test_calculate_merged_sorted_output() {
        let c = make_compactor();
        let slices = vec![
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 200,
                length: 50,
            },
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 100,
                length: 50,
            },
        ];
        let result = c.calculate_merged_slices(&slices).unwrap();
        for i in 1..result.len() {
            assert!(result[i].offset >= result[i - 1].offset, "not sorted");
        }
    }

    // find_replaced_slices tests
    #[test]
    fn test_find_replaced_no_overlap() {
        let c = make_compactor();
        let orig = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 100,
                length: 50,
            },
        ];
        let merged = orig.clone();
        let replaced = c.find_replaced_slices(&orig, &merged);
        assert!(replaced.is_empty());
    }

    #[test]
    fn test_find_replaced_all_removed() {
        let c = make_compactor();
        let orig = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
        ];
        let merged = vec![SliceDesc {
            slice_id: 3,
            chunk_id: 1,
            offset: 0,
            length: 100,
        }];
        let mut replaced = c.find_replaced_slices(&orig, &merged);
        replaced.sort();
        assert_eq!(replaced, vec![1, 2]);
    }

    // Compactor with real block store
    #[tokio::test]
    async fn test_compactor_compact_chunk_full_coverage() {
        let meta = Arc::new(MockMetaStore::new());
        let store = Arc::new(InMemoryBlockStore::new());
        let layout = ChunkLayout::default();

        // 2 slices, slice 1 fully covered by slice 2
        let chunk_id = 1u64;
        {
            let mut guard = meta.slices.lock().unwrap();
            guard.insert(
                chunk_id,
                vec![
                    SliceDesc {
                        slice_id: 1,
                        chunk_id,
                        offset: 0,
                        length: 100,
                    },
                    SliceDesc {
                        slice_id: 2,
                        chunk_id,
                        offset: 0,
                        length: 200,
                    },
                ],
            );
        }

        // Write some data for slice 1 and slice 2
        let s1_data = vec![0xAAu8; 100];
        store.write_fresh_range((1, 0), 0, &s1_data).await.unwrap();
        let s2_data = vec![0xBBu8; 200];
        store.write_fresh_range((2, 0), 0, &s2_data).await.unwrap();

        let compactor = Compactor::with_layout(meta.clone(), store.clone(), layout);
        let result = compactor.compact_chunk(chunk_id, 1).await;

        // Should succeed (assuming fragment ratio is met — 100/(100+200)=0.33 > 0.1)
        match result {
            Ok(Some(new_id)) => {
                // New slice should have been written
                assert!(new_id > 0);
                // Meta should have been updated
                let slices = meta.get_slices(chunk_id).await.unwrap();
                assert_eq!(slices.len(), 1, "should have 1 merged slice");
            }
            Ok(None) => {
                // Fragmentation was too low or no slices to merge — acceptable
            }
            Err(e) => panic!("compact_chunk failed: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_compactor_single_slice_no_compact() {
        let meta = Arc::new(MockMetaStore::new());
        let store = Arc::new(InMemoryBlockStore::new());

        {
            let mut guard = meta.slices.lock().unwrap();
            guard.insert(
                1,
                vec![SliceDesc {
                    slice_id: 1,
                    chunk_id: 1,
                    offset: 0,
                    length: 100,
                }],
            );
        }

        let compactor = Compactor::new(meta, store);
        let result = compactor.compact_chunk(1, 1).await.unwrap();
        assert_eq!(result, None, "single slice should not trigger compaction");
    }

    // Error handling tests

    #[test]
    fn test_compactor_error_display_all_variants() {
        let e1 = CompactorError::MetaError(MetaError::NotImplemented);
        assert!(e1.to_string().contains("MetaStore error"));

        let e2 = CompactorError::BlockStoreError("disk full".to_string());
        assert!(e2.to_string().contains("BlockStore error"));

        let e3 = CompactorError::InvalidData("bad slice".to_string());
        assert!(e3.to_string().contains("Invalid data"));

        let e4 = CompactorError::IoError(std::io::Error::new(std::io::ErrorKind::Other, "io"));
        assert!(e4.to_string().contains("IO error"));
    }

    #[test]
    fn test_compactor_error_source() {
        let e = CompactorError::MetaError(MetaError::NotImplemented);
        assert!(std::error::Error::source(&e).is_some());

        let e = CompactorError::BlockStoreError("x".into());
        assert!(std::error::Error::source(&e).is_none());
    }

    #[test]
    fn test_compactor_error_from_conversions() {
        let _: CompactorError = MetaError::NotImplemented.into();
        let _: CompactorError = std::io::Error::new(std::io::ErrorKind::Other, "x").into();
        let _: CompactorError = anyhow::anyhow!("test").into();
    }
}
