// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/stat.h`

#![allow(clippy::module_name_repetitions)]

use crate::mode_t;

pub const STAT_HAVE_NSEC: i32 = 1;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct stat_t {
    /// Device.
    pub st_dev: usize,
    /// File serial number.
    pub st_ino: usize,
    /// File mode.
    pub st_mode: mode_t,
    /// Link count.
    pub st_nlink: u32,
    /// User ID of the file's owner.
    pub st_uid: u32,
    /// Group ID of the file's group.
    pub st_gid: u32,
    /// Device number, if device.
    pub st_rdev: usize,
    __pad1: usize,
    /// Size of file, in bytes.
    pub st_size: isize,
    /// Optimal block size for I/O.
    pub st_blksize: i32,
    __pad2: i32,
    /// Number 512-byte blocks allocated.
    pub st_blocks: isize,
    /// Time of last access.
    pub st_atime: isize,
    pub st_atime_nsec: usize,
    /// Time of last modification.
    pub st_mtime: isize,
    pub st_mtime_nsec: usize,
    /// Time of last status change.
    pub st_ctime: isize,
    pub st_ctime_nsec: usize,
    __unused4: u32,
    __unused5: u32,
}

/// This matches struct stat64 in glibc2.1. Only used for 32 bit.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct stat64_t {
    /// Device.
    pub st_dev: u64,
    /// File serial number.
    pub st_ino: u64,
    /// File mode.
    pub st_mode: u32,
    /// Link count.
    pub st_nlink: u32,
    /// User ID of the file's owner.
    pub st_uid: u32,
    /// Group ID of the file's group.
    pub st_gid: u32,
    /// Device number, if device.
    pub st_rdev: u64,
    __pad1: u64,
    /// Size of file, in bytes.
    pub st_size: i64,
    /// Optimal block size for I/O.
    pub st_blksize: i32,
    __pad2: i32,
    /// Number 512-byte blocks allocated.
    pub st_blocks: i64,
    /// Time of last access.
    pub st_atime: i32,
    pub st_atime_nsec: u32,
    /// Time of last modification.
    pub st_mtime: i32,
    pub st_mtime_nsec: u32,
    /// Time of last status change.
    pub st_ctime: i32,
    pub st_ctime_nsec: u32,
    __unused4: u32,
    __unused5: u32,
}
