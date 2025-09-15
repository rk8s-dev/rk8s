// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/mips/include/uapi/asm/stat.h`

use crate::{gid_t, mode_t, off_t, uid_t};

/// The memory layout is the same as of struct stat64 of the 32-bit kernel.
#[repr(C)]
#[derive(Clone, Default, Debug)]
pub struct stat_t {
    pub st_dev: u32,
    /// Reserved for st_dev expansion
    st_pad0: [u32; 3],

    pub st_ino: usize,

    pub st_mode: mode_t,
    pub st_nlink: u32,

    pub st_uid: uid_t,
    pub st_gid: gid_t,

    pub st_rdev: u32,
    /// Reserved for st_rdev expansion
    st_pad1: [u32; 3],

    pub st_size: off_t,

    /// Actually this should be timestruc_t st_atime, st_mtime and st_ctime
    /// but we don't have it under Linux.
    pub st_atime: u32,
    pub st_atime_nsec: u32,

    pub st_mtime: u32,
    pub st_mtime_nsec: u32,

    pub st_ctime: u32,
    pub st_ctime_nsec: u32,

    pub st_blksize: u32,
    st_pad2: u32,

    pub st_blocks: usize,
}

pub const STAT_HAVE_NSEC: i32 = 1;
