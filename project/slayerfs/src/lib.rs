// Library crate for SlayerFS: re-export internal modules for reuse by examples and external bins.
#![allow(dead_code)]
#![allow(clippy::upper_case_acronyms)]

#[cfg(all(feature = "jemalloc", target_os = "linux"))]
use tikv_jemallocator::Jemalloc;
#[cfg(all(feature = "jemalloc", target_os = "linux"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[allow(unused_imports)]
pub mod cadapter;
pub mod chuck;
pub mod daemon;
pub mod fuse;
pub mod meta;
pub mod vfs;

pub(crate) mod utils;
