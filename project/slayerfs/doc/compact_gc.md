# slice compact and gc design

this document describes the slice compaction and garbage collection (gc) mechanisms in slayerfs.

## overview

slayerfs uses a write-once-read-many approach for data storage. when files are written, data is appended as slices within chunks. over time, this leads to:

- multiple overlapping slices for the same data range
- accumulation of outdated slice metadata
- increased storage costs from orphaned objects

the compact and gc mechanisms address these issues by merging overlapping slices and cleaning up obsolete data.

## slice compact mechanism

### purpose

compact reduces metadata overhead and improves read performance by merging overlapping or adjacent slices within the same chunk.

### when to compact

compact is triggered based on configurable thresholds:

| parameter | default value | description |
|-----------|---------------|-------------|
| min_slice_count | 5 | minimum number of slices to trigger compact |
| min_fragment_ratio | 0.1 | minimum fragmentation ratio to trigger compact |
| async_threshold | 100 | slice count threshold for async compact |
| sync_threshold | 350 | slice count threshold for sync compact |

compact is triggered when:
- slice count >= min_slice_count (5) and
- fragmentation ratio >= min_fragment_ratio (0.1)

compact mode selection:
- slice count >= min_slice_count (5) and < sync_threshold (350): async compact (background, non-blocking)
- slice count >= sync_threshold (350): sync compact (intended to block writes to the chunk)

note: 
- there are two modes (async/sync), not three. the async mode handles all cases from min_slice_count up to sync_threshold.
- **sync compact blocking writes is not yet fully implemented** - chunk-level locking mechanism needed

### fragmentation ratio calculation

```
fragmentation_ratio = (total_slice_size - merged_slice_size) / total_slice_size
```

example:
- slice a: offset 0, length 100
- slice b: offset 50, length 100 (overlaps with slice a by 50 bytes)
- total slice size: 200 (100 + 100)
- merged slice size: 150 (0-150, after removing overlap)
- fragmentation ratio: (200 - 150) / 200 = 0.25

note: this measures the amount of overlapping data between slices, not the physical layout fragmentation. higher ratio means more redundant data that can be eliminated by compaction.

### compact process

1. identify chunks meeting compact thresholds
2. select slices to compact => overlapping or adjacent
3. merge slices into fewer, larger slices
4. write new slices to object storage
5. update metadata atomically => new slices in, old slices marked delayed
6. old slices are soft-deleted => recorded in delayed_slice table

### merge algorithm

slices are merged by **removing only fully-covered slices**:

- a slice is removed only when a **single newer slice** (higher `slice_id`) completely covers its `[offset, offset+length)` range
- **partially overlapping slices are always kept intact** — their `offset` and `length` are never modified

**why no splitting?** block data is stored at `(slice_id, block_index)` where `block_index` is relative to the slice's original `offset`. if a slice's `offset` or `length` were changed (e.g., by trimming the head), the block index mapping would break — reads would address the wrong blocks. example:

```
slice A: offset=0, length=200  →  blocks at (A,0), (A,1), ...
if trimmed to offset=50, length=150  →  reader computes block_index relative to offset=50
but the actual data is still stored at (A,0) which was relative to offset=0  →  wrong data!
```

this is why the algorithm only does **whole-slice removal**, never splitting/trimming.

#### delayed data format

when old slices are soft-deleted, they are serialized into a binary blob (20 bytes per slice):

| field     | size   | encoding       |
|-----------|--------|----------------|
| slice_id  | 8 bytes | u64 little-endian |
| offset    | 8 bytes | u64 little-endian |
| size      | 4 bytes | u32 little-endian |

this format is produced by `prepare_delayed_data()` and consumed by `cleanup_delayed_slices()`.

## slice gc mechanism

### purpose

gc permanently removes obsolete slice metadata and cleans up corresponding object storage data after new slices are confirmed active.

### two-phase deletion

to ensure data safety, gc uses a two-phase deletion approach:

#### phase 1: soft delete (delayed deletion)

when compact replaces old slices:
1. new slices are written and metadata committed
2. old slices are recorded in `delayed_slice` table with timestamp
3. old slices remain in `slice_meta` table (still readable)

#### phase 2: hard delete (permanent deletion)

gc worker periodically processes delayed slices:
1. query delayed slices older than max_age_secs (default 1 hour)
2. verify new slices are active and readable
3. delete old slices from `slice_meta` table
4. remove entries from `delayed_slice` table

note: object storage data cleanup is handled separately by the object gc worker, which scans for orphaned objects not referenced by any slice metadata.

### gc worker configuration

block-level gc (blockstoregc in `src/chuck/gc.rs`):

| parameter | default value | description |
|-----------|---------------|-------------|
| interval | 3600s | gc check interval |
| batch_size | 1000 | maximum slices to process per run |
| min_age_secs | 3600 | minimum age before hard deletion |
| block_size | 4mb | block size for calculating blocks to delete |

object-level gc (markbasedgarbagecollector in `src/daemon/worker.rs`):

| parameter | default value | description |
|-----------|---------------|-------------|
| interval_secs | 3600 | gc check interval in seconds |
| batch_size | 100 | maximum files to process per run |
| max_age_secs | 3600 | minimum age before hard deletion |

## configuration

compact and gc parameters are configured via `CompactConfig`:

```rust
// CompactConfig in src/meta/config.rs
pub struct CompactConfig {
    pub min_slice_count: usize,       // default: 5
    pub min_fragment_ratio: f64,      // default: 0.1
    pub async_threshold: usize,       // default: 100
    pub sync_threshold: usize,        // default: 350
    pub interval: Duration,           // default: 1 hour
    pub max_chunks_per_run: usize,    // default: 1000
    pub max_concurrent_tasks: usize,  // default: 4 (config added, concurrent execution TODO)
}

// BlockGcConfig in src/chuck/gc.rs (for block-level GC)
pub struct BlockGcConfig {
    pub interval: Duration,          // default: 1 hour
    pub min_age_secs: i64,           // default: 1 hour
    pub batch_size: usize,           // default: 1000
    pub block_size: u64,             // default: 4MB
}

// ObjectGcConfig in src/daemon/worker.rs (for object-level GC)
pub struct ObjectGcConfig {
    pub interval_secs: u64,          // default: 1 hour
    pub batch_size: usize,           // default: 100
    pub max_age_secs: i64,           // default: 1 hour
    pub layout: ChunkLayout,
}
```

## consistency guarantees

### read consistency during compact

- reads always use the latest slice metadata
- old slices remain readable until hard deleted
- no read interruption during compact

### write consistency during compact

- async compact: does not block writes
- sync compact: **blocking writes not yet implemented** - requires chunk-level locking
- atomic metadata updates ensure no data loss

### data safety

- soft deletion ensures rollback capability
- minimum age requirement prevents premature deletion
- verification before hard deletion ensures new data is accessible

## background workers

### compactworker

- runs periodically (configurable interval)
- scans chunks for compact candidates
- executes compact in background
- logs compact statistics

### gc workers

**blockstoregc** (`src/chuck/gc.rs`):
- runs periodically (configurable interval)
- processes delayed slices for hard deletion
- cleans up block storage data
- configured via `blockgcconfig`

**markbasedgarbagecollector** (`src/daemon/worker.rs`):
- handles cleanup of deleted files
- removes orphaned objects from object storage
- configured via `objectgcconfig`

## monitoring and metrics

recommended metrics to monitor:

| metric | description |
|--------|-------------|
| compact_triggered_count | number of compact operations triggered |
| compact_merged_slices | number of slices merged |
| compact_reduced_bytes | bytes saved by compact |
| gc_delayed_deleted | number of slices soft deleted |
| gc_hard_deleted | number of slices hard deleted |
| gc_object_bytes_freed | bytes freed in object storage |

## testing

the following test suites verify compact and gc functionality:

### database_store.rs — merge_slices & compact_chunk

| test | category | description |
|------|----------|-------------|
| `test_merge_slices_functionality` | merge | basic full coverage, partial overlap, and exact-range cases |
| `test_merge_slices_different_chunk_ids_rejected` | merge | rejects slices from different chunks |
| `test_merge_slices_partial_head_overlap` | merge | head overlap keeps both slices intact |
| `test_merge_slices_partial_tail_overlap` | merge | tail overlap keeps both slices intact |
| `test_merge_slices_middle_overlap` | merge | sandwich overlap keeps outer slice intact |
| `test_merge_slices_nonzero_offset_tail_overlap` | merge | non-zero offset partial overlap |
| `test_merge_slices_union_coverage_not_removed` | merge | union of two slices covers a third but neither alone does |
| `test_merge_slices_exact_same_range` | merge | exact duplicate range removes older |
| `test_merge_slices_chain_coverage` | merge | A⊂B⊂C chain |
| `test_merge_slices_adjacent_no_overlap` | merge | adjacent slices (no overlap) all kept |
| `test_merge_slices_complex_scenario` | merge | multi-slice complex scenario |
| `test_merge_slices_zero_length` | merge | zero-length slices |
| `test_merge_slices_ordering` | merge | output sorted by offset |
| `test_compact_chunk_partial_overlap_no_change` | compact | partial overlap → no fragmentation change |
| `test_compact_chunk_cascading_full_coverage` | compact | chain of full coverages |
| `test_compact_chunk_invalid_chunk_id` | compact | non-existent chunk graceful handling |
| `test_compact_chunk_single_slice_noop` | compact | single slice → no-op |
| `test_compact_chunk_zero_size` | compact | zero-size slices |
| `test_compact_chunk_out_of_bounds_slices_filtered` | compact | out-of-bounds slices |
| `test_compact_chunk_invalid_delayed_data` | compact | invalid delayed data handling |

### database_store.rs — gc

| test | category | description |
|------|----------|-------------|
| `test_soft_delete_and_gc` | gc | two-phase soft→hard deletion with 20-byte format |
| `test_gc_respects_min_age` | gc | min_age_secs honored |
| `test_gc_batch_processing` | gc | batch_size limit |
| `test_gc_batch_size_limit` | gc | exact batch boundary |
| `test_gc_returns_correct_block_cleanup_info` | gc | correct (slice_id, offset, size) tuples |
| `test_gc_empty_delayed_table` | gc | no delayed slices → no-op |

### database_store.rs — thresholds & integration

| test | category | description |
|------|----------|-------------|
| `test_min_slices_no_fragmentation` | threshold | min_slice_count met but no fragmentation |
| `test_high_fragmentation_triggers` | threshold | high fragmentation triggers compact |
| `test_compact_stats_fragmentation_ratio` | threshold | fragmentation ratio calculation |
| `test_end_to_end_compact_then_gc` | integration | full write→compact→gc→verify cycle |
| `test_end_to_end_repeated_overwrites` | integration | repeated overwrites reduce slice count |
| `test_compact_chunk_with_delay` | integration | compact with delayed deletion |
| `test_run_compact_by_threshold_multi_chunk` | integration | multi-chunk threshold scan |

### database_store.rs — edge cases

| test | category | description |
|------|----------|-------------|
| `test_cleanup_delayed_slices_invalid_length` | edge | invalid delayed data length |
| `test_cleanup_delayed_slices_empty` | edge | empty delayed data |
| `test_compact_chunk_all_filtered` | edge | all slices filtered → no-op |
| `test_delayed_data_encode_decode_roundtrip` | edge | 20-byte format encode/decode |
| `test_find_replaced_slice_ids` | edge | set difference logic |
| `test_replace_slices_for_compact` | edge | atomic metadata replacement |
| `test_merge_overlapping_slices_in_db` | edge | DB-level merge |

### compactor.rs — calculate_merged_slices & compact_chunk

| test | category | description |
|------|----------|-------------|
| `test_calculate_merged_empty` | merge | empty input |
| `test_calculate_merged_single_slice` | merge | single slice passthrough |
| `test_calculate_merged_partial_overlap_kept` | merge | partial overlap both kept |
| `test_calculate_merged_full_coverage` | merge | full coverage removes older |
| `test_calculate_merged_head_overlap_nonzero_offset` | merge | head overlap non-zero offset |
| `test_calculate_merged_chain` | merge | A⊂B⊂C chain removal |
| `test_calculate_merged_disjoint` | merge | disjoint all kept |
| `test_calculate_merged_sandwich` | merge | middle overlap keeps outer |
| `test_calculate_merged_exact_overlap` | merge | exact overlap removes older |
| `test_calculate_merged_sorted_output` | merge | output sorted by offset |
| `test_find_replaced_no_overlap` | util | no replacements |
| `test_find_replaced_all_removed` | util | all older slices replaced |
| `test_compactor_compact_chunk_full_coverage` | compact | end-to-end with block store |
| `test_compactor_single_slice_no_compact` | compact | single slice no-op |
| `test_compactor_error_display_all_variants` | error | error Display impl |
| `test_compactor_error_source` | error | error source() chain |
| `test_compactor_error_from_conversions` | error | From impls |

### gc.rs — BlockStoreGC

| test | category | description |
|------|----------|-------------|
| `test_delete_slice_blocks_zero_size` | gc | zero size → no-op |
| `test_delete_slice_blocks_single_block` | gc | single block deletion |
| `test_delete_slice_blocks_multiple_blocks` | gc | multi-block deletion |
| `test_delete_slice_blocks_offset_ignored` | gc | offset not used for block indexing |
| `test_delete_slice_blocks_partial_last_block` | gc | partial last block (div_ceil) |
| `test_gc_cycle_no_delayed_slices` | gc-cycle | empty queue no-op |
| `test_gc_cycle_processes_slices` | gc-cycle | processes and deletes blocks |
| `test_gc_cycle_batch_respects_limit` | gc-cycle | batch_size honored |
| `test_gc_cycle_meta_error_propagated` | gc-cycle | meta error propagation |
| `test_gc_cycle_multiple_rounds` | gc-cycle | multi-round draining |
| `test_gc_error_display` | error | GCError Display |
| `test_gc_error_source` | error | GCError source chain |
| `test_gc_error_from_meta` | error | From<MetaError> |
| `test_block_gc_config_default` | config | default values |

## best practices

1. **tune thresholds based on workload**
   - high-write workloads: lower min_slice_count, more frequent compact
   - read-heavy workloads: higher thresholds, less frequent compact

2. **monitor fragmentation ratio**
   - regularly check chunk statistics
   - adjust min_fragment_ratio based on observed patterns

3. **gc timing**
   - schedule gc during low-traffic periods
   - ensure max_age_secs allows sufficient verification time

4. **storage planning**
   - account for temporary storage overhead during compact
   - delayed slices occupy metadata space until gc completes
