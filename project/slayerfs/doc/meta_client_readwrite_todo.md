# MetaClient Read/Write Parity Todo

## Goals
- Track the remaining work needed to bring the Rust `MetaClient` in line with JuiceFS `baseMeta` for core read/write behaviour.

## Task Buckets

### 1. API Surface & Behaviour Parity
- [x] Audit `MetaClient` public methods vs. Go `baseMeta` (open/close/read/write/truncate/statfs) and document missing or stubbed ones.
- [x] Draft `MetaStore` extension plan and supporting type list (see `doc/meta_client_api_extension_plan.md`).
- [x] Record one-to-one API mapping and highlight duplicates/gaps (`doc/meta_client_api_mapping.md`).
- [x] Implement `set_attr`/`open`/`close`/`link`/`stat_fs`/`symlink` on the database store backend with inline symlink target storage; track follow-up work for other backends.
- [x] Wire `read_symlink`/readlink support through MetaClient + VFS once backends expose targets consistently (DatabaseMetaStore wired; other backends pending target storage).
- [ ] Define Rust equivalents for session-aware helpers (atime update, handle caching, symlink cache) and decide scope for initial milestone.
- [ ] Specify error and permission semantics (e.g., `EROFS`, `EDQUOT`) to match JuiceFS expectations.

### 2. Read Path Enhancements
- [ ] Introduce an open-file/handle cache (like `openfiles`) to reuse metadata and implement atime refresh hooks.
- [ ] Extend cached read helpers to fetch slice lists, cache directory batches, and respect case-insensitive fallbacks.
- [ ] Plan metrics/observability for read latency (counter/histogram equivalents) and integration with tracing.

### 3. Write Path Safeguards
- [ ] Implement quota, ACL, and immutable flag checks before mutating operations; identify supporting store APIs required.
- [ ] Ensure post-write cache and statistics updates (inode cache, dir stats, user/group quotas) mirror Go logic.
- [ ] Map trash/rename/delete flows, including delayed slice cleanup triggers and sustained inode handling.

### 4. Background Maintenance
- [ ] Hook session heartbeat, stale-session cleanup, slice deletion workers, and trash GC into a unified task coordinator.
- [ ] Define configuration knobs (heartbeat, max deletes, background toggles) and wire into `MetaClientOptions`.
- [ ] Draft plan for periodic StatFS refresh and Prometheus-compatible metrics publishers.

### 5. Concurrency & Locking Strategy
- [ ] Design inode/transaction locking primitives (equivalent to `txlocks`) using Rust concurrency tools.
- [ ] Document lock acquisition order and add instrumentation to detect contention.
- [ ] Prototype batch-lock helper(s) for rename and other multi-inode operations.

### 6. Testing & Verification
- [ ] Add integration tests covering concurrent read/write, case-insensitive lookup, and quota enforcement.
- [ ] Create mock or in-memory MetaStore implementations for deterministic testing of background jobs.
- [ ] Establish benchmarking harness (similar workload to JuiceFS) to measure parity and regressions.

### 7. Documentation & Rollout
- [ ] Update developer docs describing new options, background tasks, and operational considerations.
- [ ] Prepare migration notes for existing consumers (configuration defaults, feature flags).
- [ ] Track open questions / blockers discovered during implementation and review cadence.

---
_Last updated: 2025-11-13_
