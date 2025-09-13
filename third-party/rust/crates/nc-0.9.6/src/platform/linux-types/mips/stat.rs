// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/mips/include/uapi/asm/stat.h`

use crate::{gid_t, ino_t, mode_t, off_t, time_t, uid_t};

#[repr(C)]
#[derive(Debug, Default)]
pub struct stat_t {
    pub st_dev: u32,
    /// Reserved for network id
    st_pad1: [isize; 3],
    pub st_ino: ino_t,
    pub st_mode: mode_t,
    pub st_nlink: u32,
    pub st_uid: uid_t,
    pub st_gid: gid_t,
    pub st_rdev: u32,
    st_pad2: [isize; 2],
    pub st_size: off_t,
    st_pad3: isize,

    /// Actually this should be timestruc_t st_atime, st_mtime and st_ctime
    /// but we don't have it under Linux.
    pub st_atime: time_t,
    pub st_atime_nsec: isize,
    pub st_mtime: time_t,
    pub st_mtime_nsec: isize,
    pub st_ctime: time_t,
    pub st_ctime_nsec: isize,
    pub st_blksize: isize,
    pub st_blocks: isize,
    st_pad4: [isize; 14],
}

/// This matches struct stat64 in glibc2.1, hence the absolutely insane
/// amounts of padding around dev_t's.  The memory layout is the same as of
/// struct stat of the 64-bit kernel.

#[repr(C)]
#[derive(Debug, Default)]
pub struct stat64_t {
    pub st_dev: usize,
    /// Reserved for st_dev expansion  
    st_pad0: [usize; 3],

    pub st_ino: u64,

    pub st_mode: mode_t,
    pub st_nlink: u32,

    pub st_uid: uid_t,
    pub st_gid: gid_t,

    pub st_rdev: usize,

    /// Reserved for st_rdev expansion  
    st_pad1: [usize; 3],

    pub st_size: i64,

    /// Actually this should be timestruc_t st_atime, st_mtime and st_ctime
    /// but we don't have it under Linux.
    pub st_atime: time_t,

    /// Reserved for st_atime expansion
    pub st_atime_nsec: usize,

    pub st_mtime: time_t,
    /// Reserved for st_mtime expansion
    pub st_mtime_nsec: usize,

    pub st_ctime: time_t,
    /// Reserved for st_ctime expansion
    pub st_ctime_nsec: usize,

    pub st_blksize: usize,
    st_pad2: usize,

    pub st_blocks: i64,
}

pub const STAT_HAVE_NSEC: i32 = 1;
