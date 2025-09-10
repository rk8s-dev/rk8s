// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/statfs.h`

use crate::fsid_t;

/// Most 64-bit platforms use 'long', while most 32-bit platforms use '__u32'.
///
/// Yes, they differ in signedness as well as size.
/// Special cases can override it for themselves -- except for S390x, which
/// is just a little too special for us. And MIPS, which I'm not touching
/// with a 10' pole.
#[allow(clippy::module_name_repetitions)]
pub type _statfs_word = isize;

#[allow(clippy::module_name_repetitions)]
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct statfs_t {
    pub f_type: isize,
    pub f_bsize: isize,
    pub f_blocks: isize,
    pub f_bfree: isize,
    pub f_bavail: isize,
    pub f_files: isize,
    pub f_ffree: isize,
    pub f_fsid: isize,
    pub f_namelen: isize,
    pub f_frsize: isize,
    pub f_flags: isize,
    pub f_spare: [isize; 4],
}

/// ARM needs to avoid the 32-bit padding at the end, for consistency
/// between EABI and OABI
#[allow(clippy::module_name_repetitions)]
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct statfs64_t {
    pub f_type: isize,
    pub f_bsize: isize,
    pub f_blocks: u64,
    pub f_bfree: u64,
    pub f_bavail: u64,
    pub f_files: u64,
    pub f_ffree: u64,
    pub f_fsid: fsid_t,
    pub f_namelen: isize,
    pub f_frsize: isize,
    pub f_flags: isize,
    pub f_spare: [isize; 4],
}

/// IA64 and `x86_64` need to avoid the 32-bit padding at the end,
/// to be compatible with the i386 ABI
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct compat_statfs64_t {
    pub f_type: u32,
    pub f_bsize: u32,
    pub f_blocks: u64,
    pub f_bfree: u64,
    pub f_bavail: u64,
    pub f_files: u64,
    pub f_ffree: u64,
    pub f_fsid: fsid_t,
    pub f_namelen: u32,
    pub f_frsize: u32,
    pub f_flags: u32,
    pub f_spare: [u32; 4],
}
