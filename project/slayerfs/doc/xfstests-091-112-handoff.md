# SlayerFS xfstests Handoff: generic/091 and generic/112

## Scope

This document summarizes the remaining xfstests failures after several targeted fixes in SlayerFS.
It is written as a handoff note for another LLM or engineer.

Current focus:

- `generic/091`
- `generic/112`

The latest inspected artifact is:

- `docker/compose-xfstests/artifacts/run-1776428877-9159`

Validation command expected by `AGENTS.md`:

```bash
bash docker/compose-xfstests/run_redis_xfstests.sh --cases "generic/091 generic/112"
```

## Current status

Latest run still fails only these two cases:

- `generic/091`
- `generic/112`

Relevant artifact files:

- `docker/compose-xfstests/artifacts/run-1776428877-9159/report.md`
- `docker/compose-xfstests/artifacts/run-1776428877-9159/slayerfs.log`
- `docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/091.out.bad`
- `docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/091.full`
- `docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/112.out.bad`
- `docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/112.0.fsxlog`
- `docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/112.2.full`

## Important update from the latest run

`generic/112` changed shape compared with earlier runs:

- Earlier analysis focused on `112.0`
- In the latest artifact, `112.0` finishes with `All 1000 operations completed A-OK!`
- The remaining `generic/112` failure is now from `112.2`

This means at least one earlier failure mode in `112` was likely improved, but another one remains.

## What the logs do and do not show

`slayerfs.log` in the latest artifact does not show a useful non-GC error for the failing cases.

Observed warnings:

- `MetaClient: failed to auto-start session: Internal error: sid has been set`
- `Failed to cleanup orphan uncommitted slices, error: Not implemented`

These warnings were already present before and do not directly explain the current `091`/`112` corruption signatures.

No strong error stack trace was found via grep of `error|ERROR|failed`.

## generic/091 failure signature

Source files:

- `docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/091.out.bad`
- `docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/091.full`

Final failure:

- `READ BAD DATA: offset = 0x34000, size = 0x7000`
- Expected non-zero data
- Actual data is zero-filled from the failing region start

Latest critical operation sequence near the failure:

```text
202: TRUNCATE DOWN  from 0x6f000 to 0x15000
203: TRUNCATE UP    from 0x15000 to 0x50000
205: WRITE          0x28000..0x36fff
206: WRITE          0x32000..0x3afff
207: TRUNCATE DOWN  from 0x50000 to 0x49000
208: WRITE          0x50000..0x51fff HOLE
209: READ           0x34000..0x3afff   => BAD DATA
```

Interpretation:

- The failing range `0x34000..0x3afff` is fully below the final size `0x49000`
- It was explicitly rewritten after the truncate-down/truncate-up pair
- The final read returns zeros where data from op `206` should still exist

This is not primarily a copy-range bug in this reduced trace.

## generic/112 failure signature

Source files:

- `docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/112.out.bad`
- `docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/112.2.full`
- `docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/112.0.fsxlog`

`112.out.bad` shows:

- `fsx.0` passed
- `fsx.1` passed
- `fsx.2` failed and points to `112.2.full`

Latest failing tail in `112.2.full`:

```text
5606: MAPWRITE 0x4add2d..0x4b79db
5607: MAPREAD  0x4abf9f..0x4b9d2d
...
5673: TRUNCATE DOWN from 0x90eb74 to 0x5891e6
5674: WRITE         0x5b6897..0x5bb4fc HOLE
...
5679: MAPWRITE      0x47f16d..0x484f71
...
5685: MAPREAD       0x4a54d2..0x4b12b8   => BAD DATA
```

Interpretation:

- The failing region is not obviously beyond EOF after the final truncate
- The bad read is a `MAPREAD`, not a normal `read`
- The last visible relevant write before failure is a `MAPWRITE`
- This keeps the FUSE page-cache / mmap-writeback interaction hypothesis alive

The newest `112` failure is less clearly a pure truncate-prefix preservation bug than the earlier `112.0` failure was.
It now looks more like `mmap` visibility or writeback ordering around truncates and cached pages.

## What has already been verified

### 1. Core VFS truncate regression tests pass

Local regression tests were added and pass:

- `test_fs_truncate_then_rewrite_range_stays_visible`
- `test_fs_truncate_keeps_prefix_of_overlapping_slice`
- `test_fs_truncate_extend_does_not_return_stale_reader_cache`
- `test_fs_truncate_prunes_chunks_and_zero_fills`

Command used:

```bash
cargo test -p slayerfs --lib test_fs_truncate_ --target-dir /tmp/slayerfs-target -- --nocapture
```

Result: all pass.

Consequence:

- The remaining bug likely does not reproduce in the pure library/VFS path
- The remaining issue is more likely FUSE-facing, kernel-cache-facing, or specific to mmap/writeback behavior

### 2. The latest `ReplyOpen.flags` fix did not solve the remaining failures

Recent targeted fix:

- `src/fuse/mod.rs`
- `fuse.open()` now returns `ReplyOpen { fh, flags: 0 }`

Reason:

- `ReplyOpen.flags` must contain `FOPEN_*`, not user `O_*`
- Previously returning raw `O_WRONLY/O_RDWR` could accidentally enable `FOPEN_DIRECT_IO` or `FOPEN_KEEP_CACHE`

This was a correct fix, but it was not sufficient to clear `091` and `112`.

### 3. `generic/112` improved partially

The old `112.0` style failure no longer reproduces in the latest artifact.
Only `112.2` remains.

That suggests some previous fixes improved visibility/cache behavior, but not enough.

## Fixes already made in this branch

Do not blindly revert these. They were introduced to fix earlier xfstests regressions.

1. `generic/006`
- FUSE readdir offset handling fix
- File: `src/fuse/mod.rs`

2. `generic/023` and `generic/035`
- Redis rename / hardlink / directory semantics fixes
- Files:
  - `src/meta/stores/redis/mod.rs`
  - `src/meta/client/mod.rs`

3. `generic/075`
- Redis slice rewrite / truncate atomicity fixes
- File: `src/meta/stores/redis/mod.rs`

4. Slice cache stale overwrite mitigation
- Files:
  - `src/meta/client/cache.rs`
  - `src/meta/client/mod.rs`

5. Immediate write visibility tightening
- Flush writer before write syscall returns
- Files:
  - `src/vfs/handles.rs`
  - `src/vfs/fs/mod.rs`

6. `generic/131`
- FUSE file lock end semantics fix
- File: `src/fuse/mod.rs`

7. Mount tweak
- Added `sync` mount option while keeping `write_back(true)`
- File: `src/fuse/mount.rs`

8. Redis truncate lost-update mitigation
- `rewrite_trimmed_slices` now uses `WATCH` + retry
- File: `src/meta/stores/redis/mod.rs`

9. Writeback-cache guessed-fh mitigation
- `FUSE_WRITE_CACHE` writes are routed to `write_ino()`
- `write_ino()` takes inode write locks and flushes before return
- Files:
  - `src/fuse/mod.rs`
  - `src/vfs/fs/mod.rs`

10. Internal `copy_file_range`
- Added VFS-level implementation plus FUSE callback
- Files:
  - `src/vfs/fs/mod.rs`
  - `src/fuse/mod.rs`
  - `src/vfs/fs/tests.rs`

11. Immediate timestamp updates after write
- Files:
  - `src/vfs/fs/mod.rs`

12. `ReplyOpen.flags` correction
- Return only FUSE open flags, not raw `O_*`
- File:
  - `src/fuse/mod.rs`

## Strongest current hypotheses

### Hypothesis A: mmap dirty-page / truncate ordering issue in the FUSE path

Why it is plausible:

- `112` now fails in `MAPWRITE` + `MAPREAD`
- Library-level truncate tests pass
- `write_ino()` and normal handle writes flush synchronously, but mmap writes come from kernel writeback semantics rather than the ordinary write syscall path
- A stale or partially invalidated page-cache path can still produce read-zero or stale-data behavior

Places to inspect:

- `src/fuse/mod.rs`
- `src/vfs/fs/mod.rs`
- `src/vfs/handles.rs`
- `src/vfs/io/writer.rs`

Specific questions:

- Are `setattr(size=...)`, `flush`, `fsync`, and `release` sufficiently serialized with `FUSE_WRITE_CACHE` writes?
- Are we missing an invalidation or ordering guarantee after a truncate visible to mmap-backed cached pages?
- Does the current combination of `write_back(true)` plus `sync` still allow a stale page state that SlayerFS does not notice?

### Hypothesis B: truncate plus subsequent rewrite is still racing with cached state in FUSE, not in the core metadata logic

Why it is plausible:

- `091` still shows rewrite-after-truncate data returning zeros
- Core regression tests for this class currently pass
- The failure is consistent with visibility loss between user-visible reads and the kernel/FUSE cached state

Specific questions:

- After `set_attr(size=...)` / `truncate_inode()`, do all open handles and inode-based writes see the same fresh state?
- Could a read path still observe a stale reader or stale kernel cache even though `self.state.reader.invalidate_all()` runs?
- Is there a missing synchronization point between FUSE `setattr` and subsequent buffered reads on existing mappings?

### Hypothesis C: there is still a metadata/object visibility corner case only exposed through kernel writeback batching

Why it is plausible:

- `slayerfs.log` is mostly quiet, which suggests the failure may be semantic rather than an explicit runtime error
- `mmap` writes can reorder exposure differently from ordinary writes
- Trimmed slices, hole handling, and post-truncate rewrites have already needed multiple fixes in Redis metadata code

Places to inspect:

- `src/meta/stores/redis/mod.rs`
- `src/meta/client/mod.rs`
- `src/meta/client/cache.rs`

## Code areas worth reading first

- `src/fuse/mod.rs`
  - `open`
  - `write`
  - `setattr`
  - `flush`
  - `fsync`
  - `release`

- `src/vfs/fs/mod.rs`
  - `truncate_inode`
  - `set_attr`
  - `read`
  - `write`
  - `write_ino`
  - `flush_and_sync_handle`
  - `close`

- `src/vfs/handles.rs`
  - `FileHandle::write`
  - `FileHandle::flush`
  - handle-level locking

- `src/vfs/io/writer.rs`
  - background flush
  - flush gating
  - chunk commit invalidation path

- `src/meta/stores/redis/mod.rs`
  - truncate rewrite path
  - trimmed slice handling

## Concrete next steps

1. Re-run only the failing pair from repo root:

```bash
bash docker/compose-xfstests/run_redis_xfstests.sh --cases "generic/091 generic/112"
```

2. If changing code, keep the next patch minimal and aimed at one hypothesis only.

3. Highest-value next experiment:

- instrument or tighten ordering around FUSE `setattr(size=...)` and `FUSE_WRITE_CACHE` / `flush` / `release`
- especially for already-open handles and mmap/writeback traffic

4. If additional local testing is needed, prefer adding a focused regression around:

- FUSE-backed mmap write, then truncate, then mapread
- truncate-down, truncate-up, rewrite same region, then read through a still-open handle

## Useful commands for the next investigator

Inspect latest artifacts:

```bash
ls -1dt docker/compose-xfstests/artifacts/run-* | head
```

Grep non-GC errors:

```bash
rg -n "error|ERROR|failed|FAIL|BAD DATA|WARNING" docker/compose-xfstests/artifacts/run-1776428877-9159/slayerfs.log docker/compose-xfstests/artifacts/run-1776428877-9159/results
```

Read the failing traces:

```bash
tail -n 120 docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/091.full
tail -n 160 docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/112.2.full
tail -n 220 docker/compose-xfstests/artifacts/run-1776428877-9159/results/generic/112.0.fsxlog
```

Run focused library tests:

```bash
cargo test -p slayerfs --lib test_fs_truncate_ --target-dir /tmp/slayerfs-target -- --nocapture
```

## Bottom line

The remaining problem is most likely not a simple pure-VFS truncate bug anymore.

Current evidence points more strongly to one of these:

- FUSE mmap/writeback cache visibility bug
- FUSE truncate ordering bug relative to buffered or guessed-handle writes
- a remaining metadata/object visibility corner case only exposed through the kernel writeback path

The latest run narrows `112` to `112.2`, which is a useful clue and should influence the next debugging pass.
