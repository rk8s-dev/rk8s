// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/arm/include/uapi/asm/stat.h`

#![allow(clippy::module_name_repetitions)]

pub const STAT_HAVE_NSEC: i32 = 0;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct stat_t {
    pub st_dev: usize,
    pub st_ino: usize,
    pub st_mode: u16,
    pub st_nlink: u16,
    pub st_uid: u16,
    pub st_gid: u16,
    pub st_rdev: usize,
    pub st_size: usize,
    pub st_blksize: usize,
    pub st_blocks: usize,
    pub st_atime: usize,
    pub st_atime_nsec: usize,
    pub st_mtime: usize,
    pub st_mtime_nsec: usize,
    pub st_ctime: usize,
    pub st_ctime_nsec: usize,
    __unused4: usize,
    __unused5: usize,
}

/// This matches struct stat64 in glibc2.1, hence the absolutely
/// insane amounts of padding around `dev_t`.
/// Note: The kernel zero's the padded region because glibc might read them
/// in the hope that the kernel has stretched to using larger sizes.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct stat64_t {
    pub st_dev: u64,
    __pad0: [u8; 4],

    __st_ino: usize,
    pub st_mode: u32,
    pub st_nlink: u32,

    pub st_uid: usize,
    pub st_gid: usize,

    pub st_rdev: u64,
    __pad3: [u8; 4],

    pub st_size: i64,
    pub st_blksize: usize,
    /// Number 512-byte blocks allocated.
    pub st_blocks: u64,

    pub st_atime: usize,
    pub st_atime_nsec: usize,

    pub st_mtime: usize,
    pub st_mtime_nsec: usize,

    pub st_ctime: usize,
    pub st_ctime_nsec: usize,

    pub st_ino: u64,
}

pub const STAT64_HAS_BROKEN_ST_INO: i32 = 1;
