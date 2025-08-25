//! FUSE adapter and request handling
//!
//! Responsibilities:
//! - Implement the FUSE callbacks and translate POSIX filesystem operations
//!   into VFS layer calls.
//! - Provide mount and unmount helpers and manage the fuse event loop.
//! - Handle permission checks and pass appropriate errno codes back to callers.
//!
//! Important notes / TODOs:
//! - Ensure the adapter works with libfuse3 and supports high-concurrency I/O.
//! - Provide options for foreground/background mounting and debug logging.
//!
//! Submodules:
//! - `adapter`: glue code that registers FUSE callbacks and translates requests
//! - `mount`: mount/unmount helpers and CLI plumbing
pub mod adapter;
pub mod mount;

// Module implementation TODOs remain.