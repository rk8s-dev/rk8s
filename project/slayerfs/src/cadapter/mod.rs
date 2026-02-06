//! ChunkStore adapter (cAdapter)
//!
//! Submodules:
//! - `client`: high-level client API used by writer/reader code
//! - `s3`: S3-compatible adapter implementation
//!
//! Responsibilities summary:
//! - Provide an async API for put/get/delete/list of block objects.
//! - Normalize object key layout and implement retries/backoff.
//! - Expose metrics and concurrency controls for upload/download pools.
//!
pub mod client;
pub mod localfs;
pub mod s3;
// Module-level TODOs remain: implement concrete adapter logic and tests.
