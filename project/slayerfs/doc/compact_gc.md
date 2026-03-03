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

slices are merged when:
- they belong to the same chunk
- they overlap (offset < previous_end) or are adjacent (offset == previous_end)

merged slice properties:
- offset: minimum offset of all merged slices
- length: maximum end position minus minimum offset

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

the following test cases verify compact and gc functionality:

- `test_merge_slices_functionality`: verifies slice merging logic
- `test_compact_trigger_and_merge`: verifies compact execution
- `test_compact_threshold_trigger`: verifies threshold-based triggering
- `test_soft_delete_and_gc`: verifies two-phase deletion
- `test_read_correctness_after_compact`: verifies read consistency

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
