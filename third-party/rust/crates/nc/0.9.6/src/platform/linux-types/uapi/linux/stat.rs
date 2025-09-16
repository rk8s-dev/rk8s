// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/stat.h`

use crate::mode_t;

pub const S_IFMT: mode_t = 0o0_170_000;
pub const S_IFSOCK: mode_t = 0o14_0000;
pub const S_IFLNK: mode_t = 0o120_000;
pub const S_IFREG: mode_t = 0o100_000;
pub const S_IFBLK: mode_t = 0o060_000;
pub const S_IFDIR: mode_t = 0o040_000;
pub const S_IFCHR: mode_t = 0o020_000;
pub const S_IFIFO: mode_t = 0o010_000;
pub const S_ISUID: mode_t = 0o004_000;
pub const S_ISGID: mode_t = 0o002_000;
pub const S_ISVTX: mode_t = 0o001_000;

#[must_use]
#[inline]
pub const fn S_ISLNK(m: mode_t) -> bool {
    (m & S_IFMT) == S_IFLNK
}

#[must_use]
#[inline]
pub const fn S_ISREG(m: mode_t) -> bool {
    (m & S_IFMT) == S_IFREG
}

#[must_use]
#[inline]
pub const fn S_ISDIR(m: mode_t) -> bool {
    (m & S_IFMT) == S_IFDIR
}

#[must_use]
#[inline]
pub const fn S_ISCHR(m: mode_t) -> bool {
    (m & S_IFMT) == S_IFCHR
}

#[must_use]
#[inline]
pub const fn S_ISBLK(m: mode_t) -> bool {
    (m & S_IFMT) == S_IFBLK
}

#[must_use]
#[inline]
pub const fn S_ISFIFO(m: mode_t) -> bool {
    (m & S_IFMT) == S_IFIFO
}

#[must_use]
#[inline]
pub const fn S_ISSOCK(m: mode_t) -> bool {
    (m & S_IFMT) == S_IFSOCK
}

pub const S_IRWXU: mode_t = 0o0700;
pub const S_IRUSR: mode_t = 0o0400;
pub const S_IWUSR: mode_t = 0o0200;
pub const S_IXUSR: mode_t = 0o0100;

pub const S_IRWXG: mode_t = 0o0070;
pub const S_IRGRP: mode_t = 0o0040;
pub const S_IWGRP: mode_t = 0o0020;
pub const S_IXGRP: mode_t = 0o0010;

pub const S_IRWXO: mode_t = 0o0007;
pub const S_IROTH: mode_t = 0o0004;
pub const S_IWOTH: mode_t = 0o0002;
pub const S_IXOTH: mode_t = 0o0001;

/// Timestamp structure for the timestamps in struct statx.
///
/// `tv_sec` holds the number of seconds before (negative) or after (positive)
/// 00:00:00 1st January 1970 UTC.
///
/// `tv_nsec` holds a number of nanoseconds (0..999,999,999) after the `tv_sec` time.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct statx_timestamp_t {
    pub tv_sec: i64,
    pub tv_nsec: u32,
    // `reserved` is held in case we need a yet finer resolution.
    reserved: i32,
}

/// Structures for the extended file attribute retrieval system call (`statx()`).
///
/// The caller passes a mask of what they're specifically interested in as a
/// parameter to `statx()`.  What `statx()` actually got will be indicated in
/// `st_mask` upon return.
///
/// For each bit in the mask argument:
///
/// - if the datum is not supported:
///
/// - the bit will be cleared, and
///
/// - the datum will be set to an appropriate fabricated value if one is
///   available (eg. CIFS can take a default uid and gid), otherwise
///
/// - the field will be cleared;
///
/// - otherwise, if explicitly requested:
///
/// - the datum will be synchronised to the server if `AT_STATX_FORCE_SYNC` is
///   set or if the datum is considered out of date, and
///
/// - the field will be filled in and the bit will be set;
///
/// - otherwise, if not requested, but available in approximate form without any
///   effort, it will be filled in anyway, and the bit will be set upon return
///   (it might not be up to date, however, and no attempt will be made to
///   synchronise the internal state first);
///
/// - otherwise the field and the bit will be cleared before returning.
///
/// Items in `STATX_BASIC_STATS` may be marked unavailable on return, but they
/// will have values installed for compatibility purposes so that `stat()` and
/// co. can be emulated in userspace.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct statx_t {
    // 0x00
    /// What results were written (uncond)
    pub stx_mask: u32,
    /// Preferred general I/O size (uncond)
    pub stx_blksize: u32,
    /// Flags conveying information about the file (uncond)
    pub stx_attributes: u64,

    // 0x10
    /// Number of hard links
    pub stx_nlink: u32,
    /// User ID of owner
    pub stx_uid: u32,
    /// Group ID of owner
    pub stx_gid: u32,
    /// File mode
    pub stx_mode: u16,
    spare0: [u16; 1],

    // 0x20
    /// Inode number
    pub stx_ino: u64,
    /// File size
    pub stx_size: u64,
    /// Number of 512-byte blocks allocated
    pub stx_blocks: u64,
    /// Mask to show what's supported in `stx_attributes`
    pub stx_attributes_mask: u64,

    // 0x40
    /// Last access time
    pub stx_atime: statx_timestamp_t,
    /// File creation time
    pub stx_btime: statx_timestamp_t,
    /// Last attribute change time
    pub stx_ctime: statx_timestamp_t,
    /// Last data modification time
    pub stx_mtime: statx_timestamp_t,

    // 0x80
    /// Device ID of special file (if bdev/cdev)
    pub stx_rdev_major: u32,
    pub stx_rdev_minor: u32,
    /// ID of device containing file (uncond)
    pub stx_dev_major: u32,
    pub stx_dev_minor: u32,

    // 0x90/
    /// Spare space for future expansion
    spare2: [u64; 14],
    // 0x100
}

/// Flags to be `stx_mask`
///
/// Query request/result mask for `statx()` and struct `statx::stx_mask`.
///
/// These bits should be set in the mask argument of `statx()` to request
/// particular items when calling `statx()`.
/// Want/got `stx_mode & S_IFMT`
pub const STATX_TYPE: u32 = 0x0000_0001;
/// Want/got `stx_mode & ~S_IFMT`
pub const STATX_MODE: u32 = 0x0000_0002;
/// Want/got `stx_nlink`
pub const STATX_NLINK: u32 = 0x0000_0004;
/// Want/got `stx_uid`
pub const STATX_UID: u32 = 0x0000_0008;
/// Want/got `stx_gid`
pub const STATX_GID: u32 = 0x0000_0010;
/// Want/got `stx_atime`
pub const STATX_ATIME: u32 = 0x0000_0020;
/// Want/got `stx_mtime`
pub const STATX_MTIME: u32 = 0x0000_0040;
/// Want/got `stx_ctime`
pub const STATX_CTIME: u32 = 0x0000_0080;
/// Want/got `stx_ino`
pub const STATX_INO: u32 = 0x0000_0100;
/// Want/got `stx_size`
pub const STATX_SIZE: u32 = 0x0000_0200;
/// Want/got `stx_blocks`
pub const STATX_BLOCKS: u32 = 0x0000_0400;
/// The stuff in the normal stat struct
pub const STATX_BASIC_STATS: u32 = 0x0000_07ff;
/// Want/got `stx_btime`
pub const STATX_BTIME: u32 = 0x0000_0800;
/// All currently supported flags
pub const STATX_ALL: u32 = 0x0000_0fff;
/// Reserved for future struct statx expansion
pub const STATX__RESERVED: u32 = 0x8000_0000;

/// Attributes to be found in `stx_attributes` and masked in `stx_attributes_mask`.
///
/// These give information about the features or the state of a file that might
/// be of use to ordinary userspace programs such as GUIs or ls rather than
/// specialised tools.
///
/// Note that the flags marked `I` correspond to generic `FS_IOC_FLAGS`
/// semantically.  Where possible, the numerical value is picked to correspond
/// also.
/// `I` File is compressed by the fs
pub const STATX_ATTR_COMPRESSED: i32 = 0x0000_0004;
/// `I` File is marked immutable
pub const STATX_ATTR_IMMUTABLE: i32 = 0x0000_0010;
/// `I` File is append-only
pub const STATX_ATTR_APPEND: i32 = 0x0000_0020;
/// `I` File is not to be dumped
pub const STATX_ATTR_NODUMP: i32 = 0x0000_0040;
/// `I` File requires key to decrypt in fs
pub const STATX_ATTR_ENCRYPTED: i32 = 0x0000_0800;

/// Dir: Automount trigger
pub const STATX_ATTR_AUTOMOUNT: i32 = 0x0000_1000;

/// Check for file existence.
// These definitions are found in kernel headers.
pub const F_OK: i32 = 0;

/// Check file is readable.
pub const R_OK: i32 = 4;

/// Check file is writable.
pub const W_OK: i32 = 2;

/// Check file is executable.
pub const X_OK: i32 = 1;
