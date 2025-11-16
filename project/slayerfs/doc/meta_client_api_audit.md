# MetaClient vs. JuiceFS baseMeta: API Audit

_Date: 2025-11-13_

## Scope
- Focus on core read/write paths surfaced by JuiceFS `baseMeta` (`lookup`, `open`, `read`, `write`, `truncate`, `statfs`, etc.).
- Highlight whether the Rust `MetaClient` exposes equivalent behaviour today, and outline immediate follow-up actions.

## Summary Table

| Operation | `baseMeta` Behaviour (Go) | Rust `MetaClient` Status | Notes / Follow-up |
|-----------|---------------------------|--------------------------|--------------------|
| `StatFS` | Computes capacity/usage via cached counters + store refresh; updates Prometheus gauges | **Missing** (no `statfs` helper on `MetaClient`; `MetaStore` trait lacks equivalent) | Decide whether to extend `MetaStore` trait or expose standalone helper mirroring Go logic. |
| `Lookup` | Permission-aware lookup with case-insensitive fallback; updates parent cache | Implemented (`cached_lookup` + `resolve_case`) | Need to add permission checks & directory parent tracking parity. |
| `GetAttr` | Uses open-file cache (`of`), handles root/trash special cases | Partial (`cached_stat`, but no open-file cache, special root/trash handling minimal) | Introduce handle cache and mirror special inode behaviour. |
| `SetAttr` | Updates open cache, quota/accounting | **Missing** (no `set_attr` API on `MetaClient`; trait lacks) | Requires new trait method and cache/quota integration. |
| `Create` / `Mknod` | Quota checks, inode allocation, stats update, open cache | Partial (`create_file`, `mkdir` exist but lack quota/stat hooks) | Implement quota enforcement + stats update hooks. |
| `Symlink` / `ReadLink` | Path validation, cached atime handling | **Missing** (no symlink helpers today) | Need store support + cache wiring. |
| `Link` | Quota checks, stats update, cache maintenance | **Missing** | Need method plus quota/stat updates. |
| `Open` | Permission, immutable/append flag enforcement, atime touch, handle cache | **Missing** | Requires introducing handle cache + atime update logic. |
| `Close` | Handle cache cleanup, sustained inode GC | **Missing** | Depends on handle cache implementation. |
| `Read` | Uses chunk cache, triggers compaction heuristics, atime updates | **Missing** | Need chunk cache representation (probably separate module) and compaction scheduling. |
| `Write` | Chunk cache invalidation, stats/quota updates, compaction heuristics | **Missing** | Requires store API + accounting integration. |
| `Truncate` | Updates quotas, invalidates caches | **Missing** | Need to extend trait; integrate with stats/quota. |
| `Fallocate` | Various mode validations, quota updates | **Missing** | Optional for initial parity. |
| `Rename` | Complex quota/stat handling, trash integration | Partial (basic rename implemented; lacks quotas, trash logic, flags) | Extend implementation to cover quotas, trash semantics, flag handling. |
| `Unlink` / `Rmdir` | Trash handling, quota/stat updates | Partial (`unlink`/`rmdir` exist without trash/quota logic) | Port trash integration + accounting. |
| `StatFS` metrics | Registers Prometheus metrics and refresh loop | **Missing** | Decide on metrics backend; align with Rust observability stack. |
| Session lifecycle | `NewSession`, heartbeat, stale cleanup | Partial (`start_default_session`, `cleanup_stale_sessions`) | Continue wiring into higher-level lifecycle + ACL cache load, quota preload. |

## Immediate Actions

1. Propose extensions to `MetaStore` trait (or companion traits) covering the missing operations (`set_attr`, `link`, `rename` flags, `open`/`close`, data path operations).
2. Design handle cache abstraction analogous to `openfiles` to support atime updates and chunk caching.
3. Define quota/stat accounting strategy (data structures + interactions with store) prior to porting write-path logic.
4. Evaluate how much of Go's background maintenance (trash, compaction) should be part of API surface vs. separate workers in Rust.

## Open Questions

- Should Rust client expose a narrower API and delegate data-path operations to another service (e.g., `ChunkStore`) instead of mirroring Go's combined meta/data responsibilities?
- How do we reconcile Prometheus metrics in JuiceFS with the tracing/metrics stack used in `slayerfs` today?
- What is the minimal viable subset required for the first parity milestone (e.g., metadata-only parity vs. full data-path parity)?

---
