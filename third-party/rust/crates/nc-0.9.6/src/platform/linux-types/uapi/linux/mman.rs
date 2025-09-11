// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/mman.h`

use crate::{
    HUGETLB_FLAG_ENCODE_16GB, HUGETLB_FLAG_ENCODE_16MB, HUGETLB_FLAG_ENCODE_1GB,
    HUGETLB_FLAG_ENCODE_1MB, HUGETLB_FLAG_ENCODE_256MB, HUGETLB_FLAG_ENCODE_2GB,
    HUGETLB_FLAG_ENCODE_2MB, HUGETLB_FLAG_ENCODE_32MB, HUGETLB_FLAG_ENCODE_512KB,
    HUGETLB_FLAG_ENCODE_512MB, HUGETLB_FLAG_ENCODE_64KB, HUGETLB_FLAG_ENCODE_8MB,
    HUGETLB_FLAG_ENCODE_MASK, HUGETLB_FLAG_ENCODE_SHIFT,
};

pub const MREMAP_MAYMOVE: i32 = 1;
pub const MREMAP_FIXED: i32 = 2;
pub const MREMAP_DONTUNMAP: i32 = 4;

pub const OVERCOMMIT_GUESS: i32 = 0;
pub const OVERCOMMIT_ALWAYS: i32 = 1;
pub const OVERCOMMIT_NEVER: i32 = 2;

/// Share changes
pub const MAP_SHARED: i32 = 0x01;
/// Changes are private
pub const MAP_PRIVATE: i32 = 0x02;
/// share + validate extension flags
pub const MAP_SHARED_VALIDATE: i32 = 0x03;

/// Huge page size encoding when `MAP_HUGETLB` is specified, and a huge page
/// size other than the default is desired.
///
/// See `hugetlb_encode.h`.
///
/// All known huge page size encodings are provided here.
///
/// It is the responsibility of the application to know which sizes are supported
/// on the running system.
///
/// See `mmap(2)` man page for details.
pub const MAP_HUGE_SHIFT: i32 = HUGETLB_FLAG_ENCODE_SHIFT;
pub const MAP_HUGE_MASK: i32 = HUGETLB_FLAG_ENCODE_MASK;

pub const MAP_HUGE_64KB: usize = HUGETLB_FLAG_ENCODE_64KB;
pub const MAP_HUGE_512KB: usize = HUGETLB_FLAG_ENCODE_512KB;
pub const MAP_HUGE_1MB: usize = HUGETLB_FLAG_ENCODE_1MB;
pub const MAP_HUGE_2MB: usize = HUGETLB_FLAG_ENCODE_2MB;
pub const MAP_HUGE_8MB: usize = HUGETLB_FLAG_ENCODE_8MB;
pub const MAP_HUGE_16MB: usize = HUGETLB_FLAG_ENCODE_16MB;
pub const MAP_HUGE_32MB: usize = HUGETLB_FLAG_ENCODE_32MB;
pub const MAP_HUGE_256MB: usize = HUGETLB_FLAG_ENCODE_256MB;
pub const MAP_HUGE_512MB: usize = HUGETLB_FLAG_ENCODE_512MB;
pub const MAP_HUGE_1GB: usize = HUGETLB_FLAG_ENCODE_1GB;
pub const MAP_HUGE_2GB: usize = HUGETLB_FLAG_ENCODE_2GB;
pub const MAP_HUGE_16GB: usize = HUGETLB_FLAG_ENCODE_16GB;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct cachestat_range_t {
    pub off: u64,
    pub len: u64,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct cachestat_t {
    pub nr_cache: u64,
    pub nr_dirty: u64,
    pub nr_writeback: u64,
    pub nr_evicted: u64,
    pub nr_recently_evicted: u64,
}
