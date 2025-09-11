// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/fcntl.h`

use crate::F_LINUX_SPECIFIC_BASE;

pub const F_SETLEASE: u32 = F_LINUX_SPECIFIC_BASE;
pub const F_GETLEASE: u32 = F_LINUX_SPECIFIC_BASE + 1;

/// Request nofications on a directory.
/// See below for events that may be notified.
pub const F_NOTIFY: u32 = F_LINUX_SPECIFIC_BASE + 2;

/// Cancel a blocking posix lock; internal use only until we expose an
/// asynchronous lock api to userspace:
pub const F_CANCELLK: u32 = F_LINUX_SPECIFIC_BASE + 5;

/// Create a file descriptor with `FD_CLOEXEC` set.
pub const F_DUPFD_CLOEXEC: u32 = F_LINUX_SPECIFIC_BASE + 6;

/// Set and get of pipe page size array
pub const F_SETPIPE_SZ: u32 = F_LINUX_SPECIFIC_BASE + 7;
pub const F_GETPIPE_SZ: u32 = F_LINUX_SPECIFIC_BASE + 8;

/// Set/Get seals
pub const F_ADD_SEALS: u32 = F_LINUX_SPECIFIC_BASE + 9;
pub const F_GET_SEALS: u32 = F_LINUX_SPECIFIC_BASE + 10;

/// Set/Get write life time hints. `{GET,SET}_RW_HINT` operate on the
/// underlying inode, while `{GET,SET}_FILE_RW_HINT` operate only on
/// the specific file.
pub const F_GET_RW_HINT: u32 = F_LINUX_SPECIFIC_BASE + 11;
pub const F_SET_RW_HINT: u32 = F_LINUX_SPECIFIC_BASE + 12;
pub const F_GET_FILE_RW_HINT: u32 = F_LINUX_SPECIFIC_BASE + 13;
pub const F_SET_FILE_RW_HINT: u32 = F_LINUX_SPECIFIC_BASE + 14;

/// Types of seals
/// prevent further seals from being set
pub const F_SEAL_SEAL: u32 = 0x0001;
/// prevent file from shrinking
pub const F_SEAL_SHRINK: u32 = 0x0002;
/// prevent file from growing
pub const F_SEAL_GROW: u32 = 0x0004;
/// prevent writes
pub const F_SEAL_WRITE: u32 = 0x0008;
/// prevent future writes while mapped
pub const F_SEAL_FUTURE_WRITE: u32 = 0x0010;
// (1U << 31) is reserved for signed error codes

/*
 * Valid hint values for F_{GET,SET}_RW_HINT. 0 is "not set", or can be
 * used to clear any hints previously set.
 */
pub const RWF_WRITE_LIFE_NOT_SET: i32 = 0;
pub const RWH_WRITE_LIFE_NONE: i32 = 1;
pub const RWH_WRITE_LIFE_SHORT: i32 = 2;
pub const RWH_WRITE_LIFE_MEDIUM: i32 = 3;
pub const RWH_WRITE_LIFE_LONG: i32 = 4;
pub const RWH_WRITE_LIFE_EXTREME: i32 = 5;

/// Types of directory notifications that may be requested.
/// File accessed
pub const DN_ACCESS: i32 = 0x0000_0001;
/// File modified
pub const DN_MODIFY: u32 = 0x0000_0002;
/// File created
pub const DN_CREATE: u32 = 0x0000_0004;
/// File removed
pub const DN_DELETE: u32 = 0x0000_0008;
/// File renamed
pub const DN_RENAME: u32 = 0x0000_0010;
/// File changed attibutes
pub const DN_ATTRIB: u32 = 0x0000_0020;
/// Don't remove notifier
pub const DN_MULTISHOT: u32 = 0x8000_0000;

/// Special value used to indicate openat should use the current working directory.
pub const AT_FDCWD: i32 = -100;

/// Do not follow symbolic links.
pub const AT_SYMLINK_NOFOLLOW: i32 = 0x100;

/// Remove directory instead of unlinking file.
pub const AT_REMOVEDIR: i32 = 0x200;

/// Follow symbolic links.
pub const AT_SYMLINK_FOLLOW: i32 = 0x400;
/// Suppress terminal automount traversal
pub const AT_NO_AUTOMOUNT: i32 = 0x800;
/// Allow empty relative pathname
pub const AT_EMPTY_PATH: i32 = 0x1000;

/// Type of synchronisation required from `statx()`
pub const AT_STATX_SYNC_TYPE: i32 = 0x6000;
/// - Do whatever `stat()` does
pub const AT_STATX_SYNC_AS_STAT: i32 = 0x0000;
/// - Force the attributes to be sync'd with the server
pub const AT_STATX_FORCE_SYNC: i32 = 0x2000;
/// - Don't sync attributes with the server
pub const AT_STATX_DONT_SYNC: i32 = 0x4000;

/// Apply to the entire subtree
pub const AT_RECURSIVE: i32 = 0x8000;
