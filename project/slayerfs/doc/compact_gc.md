# Slice Compaction and GC Design

This document describes the slice compaction and garbage collection mechanisms in SlayerFS.

## Overview

SlayerFS uses a write-once-read-many approach for data storage. When files are written, data is appended as slices within chunks. Over time, this leads to:

- Multiple overlapping slices for the same data range
- Accumulation of outdated slice metadata
- Increased storage costs from orphaned objects

The compaction and GC mechanisms address these issues by merging overlapping slices and cleaning up obsolete data.

## Slice Compaction Mechanism

### Purpose

Compaction reduces metadata overhead and improves read performance by:

- **Light Compaction**: Removing fully-covered slices (metadata-only operation)
- **Heavy Compaction**: Rewriting all data into a single merged slice

### Compaction Types

#### Light Compaction

Metadata-only compaction that removes slices completely covered by newer slices:

1. Identify slices where a newer slice (higher `slice_id`) completely covers its `[offset, offset+length)` range
2. Record these slices in the `delayed_slice` table for later GC cleanup
3. Remove them from `slice_meta` table atomically

**Why no splitting? (Light Compaction only)** Block data is stored at `(slice_id, block_index)` where `block_index` is relative to the slice's original `offset`. If a slice's `offset` or `length` were changed, the block index mapping would break.

Example:

```
Slice A: offset=0, length=200  →  blocks at (A,0), (A,1), ...
If trimmed to offset=50, length=150:
  Reader computes block_index relative to offset=50
  But actual data stored at (A,0) relative to offset=0  →  wrong data!
```

#### Heavy Compaction

Data-rewrite compaction that merges all slices into a single new slice. Unlike Light Compaction, Heavy Compaction rewrites all data blocks, so it is not constrained by the "no splitting" limitation above:

1. Read all existing slices data
2. Merge in memory (newer slices overwrite older ones at overlaps)
3. Allocate new `slice_id` via `next_id(SLICE_ID_KEY)`
4. Record as uncommitted via `record_uncommitted_slice()`
5. Write merged data to block storage
6. Create new `SliceDesc` covering entire chunk
7. Atomically replace old slices via `replace_slices_for_compact_with_version()`
8. Confirm via `confirm_slice_committed()`
9. Cleanup on failure

### When to Compact

| Parameter              | Default | Description                        |
| ---------------------- | ------- | ---------------------------------- |
| `min_slice_count`    | 5       | Minimum slices to trigger          |
| `min_fragment_ratio` | 0.1     | Minimum fragmentation ratio        |
| `sync_threshold`     | 350     | Threshold for sync (blocking) mode |

```rust
if slice_count >= min_slice_count && frag_ratio >= min_fragment_ratio {
    is_sync = slice_count >= sync_threshold
    run_compaction
}
```

Note: `async_threshold` is defined in config but not currently used in compaction decision logic.

### Fragmentation Ratio

```
fragmentation = (total_slice_size - merged_size) / total_slice_size
```

Example: Slice A (0-100) + Slice B (50-150) overlapping by 50 bytes:

- Total: 200 bytes, Merged: 150 bytes
- Fragmentation: (200-150)/200 = 0.25

## Slice GC Mechanism

### Two-Phase Deletion

#### Phase 1: Soft Delete

During compaction:

1. Encode removed slices to 20-byte binary format
2. Store in `delayed_slice` table
3. Remove from `slice_meta`

**Delayed Data Format** (20 bytes):

| Field    | Size    | Type   |
| -------- | ------- | ------ |
| slice_id | 8 bytes | u64 LE |
| offset   | 8 bytes | u64 LE |
| size     | 4 bytes | u32 LE |

#### Phase 2: Hard Delete

GC cycle:

1. Query delayed slices older than `min_age_secs`
2. Delete block data via `delete_range()`
3. Confirm via `confirm_delayed_deleted()`

**Note on idempotency**: `process_delayed_slices()` handles retries safely:

- For `pending` records: deletes from `slice_meta`, changes status to `meta_deleted`, returns slice info
- For `meta_deleted` records: returns them again to allow block deletion retry
  This ensures block deletion failures can be retried in subsequent GC cycles.

### Orphan Cleanup

Uncommitted slices (crashed during heavy compaction):

1. Query `pending` records older than `orphan_cleanup_age_secs` AND all `orphan` records (regardless of age)
2. Delete block data
3. Remove via `delete_uncommitted_slices()`

Note: `orphan` status is set when a `pending` slice is detected to have no corresponding `slice_meta` entry (already deleted by other means).

## Configuration

```rust
// CompactConfig (in src/meta/config.rs)
pub struct CompactConfig {
    pub min_slice_count: usize,       // 5
    pub min_fragment_ratio: f64,      // 0.1
    pub async_threshold: usize,       // 100 (reserved field, currently unused in compaction logic)
    pub sync_threshold: usize,        // 350 (threshold for blocking/sync compaction)
    pub interval: Duration,           // 1 hour
    pub max_chunks_per_run: usize,    // 1000 (chunks to scan per compaction cycle)
    pub max_concurrent_tasks: usize,  // 4 (reserved for future parallel compaction)
    pub lock_ttl: LockTtlConfig,
}

// Reserved configs (defined but not yet implemented in current logic):
// - async_threshold: defined but not used in compaction decision currently
// - max_concurrent_tasks: defined but chunks are processed sequentially currently

// Note: CompactionWorkerConfig (in src/chunk/compact/worker.rs) also has
// max_chunks_per_run: usize, // default: 100 (for worker scan limit)

// LockTtlConfig
pub struct LockTtlConfig {
    pub async_ttl_secs: u64,      // 10
    pub sync_ttl_secs: u64,       // 30
    pub ttl_per_slice_ms: u64,    // 50
    pub min_ttl_secs: u64,        // 5
    pub max_ttl_secs: u64,        // 300
}

// BlockGcConfig
pub struct BlockGcConfig {
    pub interval: Duration,           // 1 hour
    pub min_age_secs: i64,            // 1 hour
    pub batch_size: usize,            // 1000
    pub block_size: u64,              // 4MB
    pub orphan_cleanup_age_secs: i64, // 1 hour
}
```

## Lock Manager

`CompactLockManager` provides:

- **Local lock**: `HashSet<u64>` for in-process fast check
- **Global lock**: MetaStore TTL lock for cross-process sync (only acquired for sync compaction)

Unlock behavior: removes local lock only; global lock expires via TTL.

## Background Workers

### CompactionWorker

Runs two background tasks:

1. **Compaction cycle**: Scans chunks, compacts those meeting thresholds
2. **GC cycle**: Runs `BlockStoreGC` to cleanup delayed and orphan slices

### BlockStoreGC

Methods:

- `new(meta_store, block_store)` - Create GC instance
- `start(config)` - Spawn background task
- `run_gc_cycle(config)` - Run single GC cycle

## Test Files

| File                                 | Description                          |
| ------------------------------------ | ------------------------------------ |
| `tests/gc_test.rs`                 | BlockStoreGC unit tests              |
| `tests/compactor_test.rs`          | Compactor unit tests                 |
| `tests/compaction_worker_test.rs`  | Lock manager and worker tests        |
| `tests/gc_compact_e2e_test.rs`     | Data integrity E2E tests             |
| `tests/compaction_perf_test.rs`    | Performance benchmarks               |
| `tests/light_compact_benchmark.rs` | Light vs Heavy compaction comparison |
| `tests/common/mod.rs`              | Shared test utilities                |

---

## Light Compact Design & Benchmark

### Why Light Compact Instead of Direct Heavy?

There are two types of overlap:

1. **Full Coverage**: A newer slice completely covers an older slice's range

   - Example: Write `[0, 100)`, then write `[0, 100)` again
   - The old slice is fully "dead" and can be removed without data rewrite
2. **Partial Overlap**: A newer slice partially overlaps an older slice

   - Example: Write `[0, 100)`, then write `[50, 150)`
   - Requires merging data, which needs read + write I/O

**The Problem with Direct Heavy Compaction**:

- Heavy compaction always rewrites all data blocks (read -> merge -> write)
- For full coverage scenarios, this is wasteful: we're reading and rewriting data that will be immediately overwritten
- Heavy compaction is expensive: O(chunk_size) I/O regardless of slice count

**Light Compact Solution**:

- Light compaction only removes fully-covered slices
- No data I/O, just atomic metadata updates: O(slices) instead of O(chunk_size)
- Fast enough to run frequently (e.g., after every write), preventing slice accumulation

### Benchmark Design

The benchmark ([result](https://github.com/Tyooughtul/rk8s/actions/runs/23654099514)) validates that Light Compact reduces Heavy Compact trigger frequency. The test simulates a continuous write workload where both control groups apply the **same Heavy thresholds** (fragment_ratio ≥ 0.5, slice_count = 30), but one group additionally runs Light Compact after each write. This measures whether Light's garbage collection delays Heavy's fragmentation threshold from being reached.

**Metrics**:

- `heavy_trigger_count`: Primary metric (should decrease with Light)
- `light_removed_slices`: How much garbage Light collected
- `peak_slice_count`: Space efficiency
- `total_duration`: Overall performance

### Benchmark Results

Configuration: 200 writes per test, 1MB file, 256KB working set, 4KB partial length.

| Full Coverage | Heavy Triggers (No Light) | Heavy Triggers (With Light) | Reduction | Speedup |
| ------------- | ------------------------- | --------------------------- | --------- | ------- |
| 70%           | 7                         | 2                           | 71.4%     | 1.70x   |
| 50%           | 7                         | 3                           | 57.1%     | 1.48x   |
| 30%           | 7                         | 3                           | 57.1%     | 1.71x   |
| 10%           | 6                         | 5                           | 16.7%     | 1.08x   |
| 0%            | 6                         | 6                           | 0%        | 1.01x   |

**Key Findings**:

- **70% Full Coverage**: Light triggers 136 times, removes 152 slices, reduces Heavy from 7 to 2 triggers
- **0% Full Coverage**: Light triggers 0 times (no false positives), overhead < 1%

> Notably, when dealing with frequent small-file writes, if the Light Compact coverage range is configured sufficiently large (e.g., covering all small files), even a single Light Compact operation can deliver an approximately **9.96× speedup**.
> Additionally, the reported 1.71× speedup actually includes overhead from cases where Light Compact was incomplete and subsequently triggered Heavy Compact. Qualitatively, Heavy Compact is typically two orders of magnitude slower than Light Compact.

### Conclusion

Light Compact's value is **proportional to Full Coverage write ratio**. In workloads with >30% Full Coverage, it reduces Heavy triggers by 50%+ and provides 1.5x+ overall speedup. In pure Partial workloads, it adds negligible overhead (< 1%) because it correctly identifies when it cannot help.
