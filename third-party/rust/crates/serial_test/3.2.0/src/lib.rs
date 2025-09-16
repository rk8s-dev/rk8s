#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![deny(unused_variables)]
#![deny(missing_docs)]
#![deny(unused_imports)]

//! # serial_test
//! `serial_test` allows for the creation of serialised Rust tests using the [serial](macro@serial) attribute
//! e.g.
//! ````
//! #[test]
//! #[serial]
//! fn test_serial_one() {
//!   // Do things
//! }
//!
//! #[test]
//! #[serial(some_key)]
//! fn test_serial_another() {
//!   // Do things
//! }
//!
//! #[test]
//! #[parallel]
//! fn test_parallel_another() {
//!   // Do parallel things
//! }
//! ````
//! Multiple tests with the [serial](macro@serial) attribute are guaranteed to be executed in serial. Ordering
//! of the tests is not guaranteed however. Other tests with the [parallel](macro@parallel) attribute may run
//! at the same time as each other, but not at the same time as a test with [serial](macro@serial). Tests with
//! neither attribute may run at any time and no guarantees are made about their timing!
//!
//! For cases like doctests and integration tests where the tests are run as separate processes, we also support
//! [file_serial](macro@file_serial)/[file_parallel](macro@file_parallel), with similar properties but based off file locking. Note that there are no
//! guarantees about one test with [serial](macro@serial)/[parallel](macro@parallel) and another with [file_serial](macro@file_serial)/[file_parallel](macro@file_parallel)
//! as they lock using different methods.
//! ````
//! #[test]
//! #[file_serial]
//! fn test_serial_three() {
//!   // Do things
//! }
//! ````
//!
//! All of the attributes can also be applied at a `mod` level and will be automagically applied to all test functions in that block
//! ````
//! #[cfg(test)]
//! #[serial]
//! mod serial_attr_tests {
//!     fn foo() {
//!         // Won't have `serial` applied, because not a test function
//!         println!("Nothing");
//!     }
//!
//!     #[test]
//!     fn test_bar() {
//!        // Will be run serially
//!     }
//!}
//! ````
//!
//! ## Feature flags
#![cfg_attr(
    feature = "docsrs",
    cfg_attr(doc, doc = ::document_features::document_features!())
)]

mod code_lock;
mod parallel_code_lock;
mod rwlock;
mod serial_code_lock;

#[cfg(feature = "file_locks")]
mod file_lock;
#[cfg(feature = "file_locks")]
mod parallel_file_lock;
#[cfg(feature = "file_locks")]
mod serial_file_lock;

#[cfg(feature = "async")]
#[doc(hidden)]
pub use parallel_code_lock::{local_async_parallel_core, local_async_parallel_core_with_return};

#[doc(hidden)]
pub use parallel_code_lock::{local_parallel_core, local_parallel_core_with_return};

#[cfg(feature = "async")]
#[doc(hidden)]
pub use serial_code_lock::{local_async_serial_core, local_async_serial_core_with_return};

#[doc(hidden)]
pub use serial_code_lock::{local_serial_core, local_serial_core_with_return};

#[cfg(all(feature = "file_locks", feature = "async"))]
#[doc(hidden)]
pub use serial_file_lock::{fs_async_serial_core, fs_async_serial_core_with_return};

#[cfg(feature = "file_locks")]
#[doc(hidden)]
pub use serial_file_lock::{fs_serial_core, fs_serial_core_with_return};

#[cfg(all(feature = "file_locks", feature = "async"))]
#[doc(hidden)]
pub use parallel_file_lock::{fs_async_parallel_core, fs_async_parallel_core_with_return};

#[cfg(feature = "file_locks")]
#[doc(hidden)]
pub use parallel_file_lock::{fs_parallel_core, fs_parallel_core_with_return};

// Re-export #[serial/parallel].
pub use serial_test_derive::{parallel, serial};

#[cfg(feature = "file_locks")]
pub use serial_test_derive::{file_parallel, file_serial};

pub use code_lock::is_locked_serially;
