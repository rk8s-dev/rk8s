---
applyTo: "project/slayerfs/**"
---

# slayerfs — Path-specific Copilot Instructions

## Scope
`slayerfs` is a **stacked/layered userspace filesystem (FUSE)** designed for large monorepos and build workflows:
- Overlay/union of multiple lower layers (read-only + writable upper)
- Fast metadata operations, reliable rename/replace, and large fan-out directories
- Safe concurrency and low kernel round-trip overhead

Assume **Rust 2021+**, **Tokio**, **tracing**, `thiserror` (libs) / `anyhow` (tools/tests), and **clap** (CLI). Use `unsafe` only with a `// SAFETY:` rationale + tests.

## Design guidance
- **Layering model**: define clear precedence (upper → lower), copy-up rules, whiteouts, and CoW semantics. Document invariants.
- **Atomicity**: implement atomic `rename(2)`/replace; ensure directory operations are crash-safe and idempotent.
- **Hot paths**: optimize `lookup`, `getattr`, `readdir`, `open/read/write`, `rename/unlink`, `create/mkdir`.
- **Caching**:
  - Attribute/entry TTLs (`attr_valid`, `entry_valid`) tuned for latency vs coherence
  - Negative dentry caching for non-existent paths
  - Consider writeback cache & big-write if compatible with data integrity
- **I/O strategy**: prefer **zero-copy** or minimized copies; support **readahead** and large `READ/WRITE` requests.
- **Consistency**: ensure upper layer reflects all mutating ops; validate cross-layer rename/unlink edge cases.
- **Platform**: target Linux first; gate macOS (macFUSE) with `cfg(target_os)`; avoid Linux-specific constants leaking into cross-platform code.

## API boundaries
- Separate **VFS facade** (FUSE op handlers) from **layer engine** (path translation, copy-up, whiteouts).
- Keep **path security** strict: reject `..`, sanitize symlinks where needed, respect mode/uid/gid.
- Use strong types for inode/handle IDs; avoid accidental overflows or reuse bugs.

## Observability
- Emit `tracing` spans with: op name, inode, path, layer hit (upper/lower), latency, bytes.
- Export metrics (op counts, histograms, cache hit rate, copy-ups, whiteouts).

## Testing
- Unit tests for path resolution, whiteouts, copy-up, rename across layers.
- Integration tests using a temp mount (`fusermount3 -o auto_unmount`) or `mount.fuse` sandbox.
- Stress/soak: concurrent rename + unlink + create in deep trees; large `readdir` fan-out.
- Fuzz/property tests (`proptest`) for path normalization and layer selection.
- Snapshot tests (`insta`) for debug listings (tree dumps), optional.

## Benchmarks
- `criterion` benches for `lookup`, `readdir`, `rename`, `read/write` on large files and directories.
- Track **kernel round-trips/op**, **p95 latency**, **throughput**, and **allocation counts**.

## Copilot tips
- Prefer **Rust implementations** showing FUSE op handlers with clean separation of concerns.
- When offering alternatives, compare: writeback vs direct-IO; TTL tuning; unified vs split metadata cache.
- Always include mount/unmount examples and safe cleanup (even on failure paths).

## Safety & security
- Do not log sensitive paths or file contents.
- Validate permissions and honor `O_NOFOLLOW` where applicable.
- Ensure robust cleanup on crash or SIGINT (remove mount, flush pending copy-ups).
