# MetaClient API Extension Plan

_Date: 2025-11-13_

## Objectives
- Identify concrete additions required on the `MetaStore` trait (or related helper traits) to unlock read/write parity with JuiceFS `baseMeta`.
- Provide initial method signatures and expectations so implementation work can proceed incrementally.

## Proposed Trait Extensions

| Category | Proposed Methods | Purpose | Notes |
|----------|------------------|---------|-------|
| Attribute Mutation | `set_attr(&self, ino: i64, changes: &SetAttrRequest, flags: SetAttrFlags) -> Result<FileAttr, MetaError>` | Update inode metadata (mode/uid/gid/times/flags) while returning updated attributes for cache refresh | `SetAttrRequest` encapsulates mask bits similar to Go `SetAttr` behaviour; `SetAttrFlags` tracks `SET_*` modifiers. |
| File Handles | `open(&self, ino: i64, flags: OpenFlags) -> Result<FileAttr, MetaError>`<br>`close(&self, ino: i64) -> Result<(), MetaError>` | Allow backend to enforce handle semantics (locks, share-state) and return authoritative attrs for cache | `OpenFlags` mirrors the subset of POSIX flags the metadata layer must honour. |
| Data Path | **Reuse existing** `read_slices` / `write_slice` / `truncate_file` signatures (confirm semantics) | Ensure slice-level IO operations meet parity requirements | Document mapping between Go `Read/Write/Truncate` and current methods before considering renames. See `doc/meta_client_api_mapping.md`. |
| Directory Ops | `link(&self, ino: i64, parent: i64, name: &str) -> Result<FileAttr, MetaError>`<br>`symlink(&self, parent: i64, name: &str, target: &str) -> Result<(i64, FileAttr), MetaError>`<br>`read_symlink(&self, ino: i64) -> Result<String, MetaError>` | Support hard-link and symlink creation semantics beyond basic create | `symlink` returns new inode + attr; `read_symlink` exposes stored targets for readlink operations. |
| StatFS / Counters | `stat_fs(&self) -> Result<StatFsSnapshot, MetaError>` | Provide capacity/inode usage snapshot for reporting | Snapshot struct should carry total/available bytes/inodes. |

## Supporting Data Structures

- `SetAttrRequest`: encapsulates mask bits, new owner/mode, timestamps, flags; convert from FUSE/Linux semantics where applicable.
- `OpenFlags`: bitflags covering traditional POSIX open modes plus append/truncate hints.
- `ChunkSlice`: start/length/ID metadata matching JuiceFS slice definition.
- `ChunkWrite`: includes offset, slice metadata, and mtime for journaling.
- `WriteOutcome`: returns updated attributes plus quota/stat deltas needed by `MetaClient`.
- `StatFsSnapshot`: aggregates `total_space`, `available_space`, `used_inodes`, `available_inodes`.

## Integration Strategy

1. **Phase 1 – Trait Stubs**: Introduce new enum/struct definitions and default `MetaStore` methods returning `MetaError::NotImplemented`. This keeps existing backends compiling while enabling incremental overrides.
2. **Phase 2 – MetaClient Wiring**: Add corresponding methods on `MetaClient` that call into the store and update caches/quotas (even if some behaviours are initially placeholders).
3. **Phase 3 – Backend Support**: Update concrete stores (`DatabaseMetaStore`, `EtcdMetaStore`, etc.) to implement the new methods. Start with minimal functionality (e.g., `stat_fs`) and expand.
4. **Phase 4 – Cleanup**: Deprecate or refactor older helper functions once the new API surface is stable.

## Open Questions

- Should data-path operations live on a separate trait (e.g., `MetaStoreData`) to allow metadata-only backends?
- Do we require transactional batching for operations like `rename` that update multiple tables, or can we rely on existing store-level transactions?
- How do we expose quota/stat deltas in a backend-agnostic manner without coupling to JuiceFS’s exact accounting logic?

## Next Steps

- Review and iterate on proposed method list with backend implementers.
- Start drafting Rust types (`SetAttrRequest`, `OpenFlags`, etc.) in `src/meta/store.rs` to unblock Phase 1.
- Prioritize implementation order based on immediate parity needs (likely `stat_fs`, `set_attr`, `open`, `read_chunks`).

---
