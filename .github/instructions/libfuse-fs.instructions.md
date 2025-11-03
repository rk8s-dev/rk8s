---
applyTo: "project/libfuse-fs/**"
---

# libfuse-fs — Path-specific Copilot Instructions

## Scope
`libfuse-fs` provides a **Rust FUSE abstraction layer** and helpers used by higher-level filesystems (e.g., `slayerfs`):
- Safe wrappers around FUSE protocol & kernel contracts
- Ergonomic traits for op handlers
- Utilities for inode/handle management, caching, and error mapping

Assume **Rust 2021+**, **Tokio** (for thread-offload if needed), **tracing**, `thiserror`/`anyhow`, and no unnecessary `unsafe`.

## API design principles
- **Stable traits** for core ops (`lookup`, `getattr`, `readdir`, `open`, `read`, `write`, `flush`, `fsync`, `create`, `mkdir`, `rename`, `unlink`, `rmdir`, `symlink`, `readlink`, `link`, `setxattr/getxattr`, `statfs`).
- **Strong typing**:
  - Newtypes for `Inode`, `Handle`, and `Generation`
  - Bitflags for open modes and capabilities
  - Structured attributes (mode, uid, gid, nlink, size, times)
- **Error mapping**: precise `io::Error ↔ errno` conversion; avoid collapsing distinct errors.
- **Contracts**:
  - Lookup count semantics, nlink rules, and handle lifetimes must be explicit.
  - TTL fields (`entry_valid`, `attr_valid`) carried across helper APIs.
- **Concurrency**:
  - Provide a clean model for blocking filesystem backends (offload to a thread pool) vs non-blocking backends.
  - Avoid deadlocks: no re-entrant calls across locks; prefer sharded or lock-free structures where practical.

## Performance hooks
- Enable **big writes**, **async read**, and **writeback cache** toggles (feature-gated).
- Expose **readahead** hints and **batching** helpers to reduce context switches.
- Optional **zero-copy** read paths (e.g., using OS buffers when available).

## Cross-platform
- Primary target: Linux (FUSE 7.x/`fusermount3`).
- Provide gated support for macOS (macFUSE) with compatibility shims.
- Isolate platform glue in `platform::*` modules; avoid leaking OS-specific constants.

## Observability
- `tracing` spans for each FUSE op with fields: opcode, inode, handle, size/offset, result, latency.
- Metrics adapters (counters, histograms) behind a small trait so callers can plug any telemetry backend.

## Testing
- Unit tests for: attribute packing/unpacking, errno mapping, TTL math, handle lifecycle, path normalization.
- Property tests (`proptest`) for edge cases (overflow, invalid flags, extreme offsets).
- Integration tests mounting a **minimal memfs** using this library (mount/unmount, CRUD, rename, xattr, statfs).
- CI safety: auto-unmount and temp directories; skip privileged tests when not available.

## Copilot tips
- Generate **library-quality** Rust with public traits, `#[non_exhaustive]` enums, and clear docs (`///` + examples).
- Provide both **sync** and **async** adapter patterns with minimal boilerplate.
- Offer code samples that show:
  - creating a filesystem struct,  
  - wiring op handlers,  
  - mounting to a temp dir with auto-unmount,  
  - graceful shutdown/cleanup.

## Safety & security
- No sensitive data in logs.
- Validate inputs (lengths, flags) at the boundary; return the correct errno.
- Ensure all resources (fds, threads) are closed on unmount; guard against double-free/double-unmount.
