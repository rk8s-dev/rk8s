// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/stat.h`

#![allow(clippy::module_name_repetitions)]

use crate::{
    blkcnt_t, blksize_t, dev_t, fflags_t, gid_t, ino_t, mode_t, nlink_t, off_t, timespec_t, uid_t,
};

#[cfg(target_arch = "x86")]
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct stat_t {
    /// inode's device
    pub st_dev: dev_t,

    /// inode's number
    pub st_ino: ino_t,

    /// number of hard links
    pub st_nlink: nlink_t,

    /// inode protection mode
    pub st_mode: mode_t,

    pub st_padding0: i16,

    /// user ID of the file's owner
    pub st_uid: uid_t,

    /// group ID of the file's group
    pub st_gid: gid_t,

    st_padding1: i32,

    /// device type
    pub st_rdev: dev_t,

    pub st_atim_ext: i32,

    /// time of last access
    pub st_atim: timespec_t,

    pub st_mtim_ext: i32,

    /// time of last data modification
    pub st_mtim: timespec_t,

    pub st_ctim_ext: i32,

    /// time of last file status change
    pub st_ctim: timespec_t,

    pub st_btim_ext: i32,

    /// time of file creation
    pub st_birthtim: timespec_t,

    /// file size, in bytes
    pub st_size: off_t,

    /// blocks allocated for file
    pub st_blocks: blkcnt_t,

    /// optimal blocksize for I/O
    pub st_blksize: blksize_t,

    /// user defined flags for file
    pub st_flags: fflags_,

    /// file generation number
    pub st_gen: u64,

    st_spare: [u64; 10],
}

#[cfg(not(target_arch = "x86"))]
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct stat_t {
    /// inode's device
    pub st_dev: dev_t,

    /// inode's number
    pub st_ino: ino_t,

    /// number of hard links
    pub st_nlink: nlink_t,

    /// inode protection mode
    pub st_mode: mode_t,

    pub st_padding0: i16,

    /// user ID of the file's owner
    pub st_uid: uid_t,

    /// group ID of the file's group
    pub st_gid: gid_t,

    st_padding1: i32,

    /// device type
    pub st_rdev: dev_t,

    /// time of last access
    pub st_atim: timespec_t,

    /// time of last data modification
    pub st_mtim: timespec_t,

    /// time of last file status change
    pub st_ctim: timespec_t,

    /// time of file creation
    pub st_birthtim: timespec_t,

    /// file size, in bytes
    pub st_size: off_t,

    /// blocks allocated for file
    pub st_blocks: blkcnt_t,

    /// optimal blocksize for I/O
    pub st_blksize: blksize_t,

    /// user defined flags for file
    pub st_flags: fflags_t,

    /// file generation number
    pub st_gen: u64,

    st_spare: [u64; 10],
}

/// set user id on execution
pub const S_ISUID: i32 = 0o00_4000;
/// set group id on execution
pub const S_ISGID: i32 = 0o00_2000;
/// sticky bit
pub const S_ISTXT: i32 = 0o00_1000;

/// RWX mask for owner
pub const S_IRWXU: i32 = 0o00_0700;
/// R for owner
pub const S_IRUSR: i32 = 0o00_0400;
/// W for owner
pub const S_IWUSR: i32 = 0o00_0200;
/// X for owner
pub const S_IXUSR: i32 = 0o00_0100;

pub const S_IREAD: i32 = S_IRUSR;
pub const S_IWRITE: i32 = S_IWUSR;
pub const S_IEXEC: i32 = S_IXUSR;

/// RWX mask for group
pub const S_IRWXG: i32 = 0o00_0070;
/// R for group
pub const S_IRGRP: i32 = 0o00_0040;
/// W for group
pub const S_IWGRP: i32 = 0o00_0020;
/// X for group
pub const S_IXGRP: i32 = 0o00_0010;

/// RWX mask for other
pub const S_IRWXO: i32 = 0o00_0007;
/// R for other
pub const S_IROTH: i32 = 0o00_0004;
/// W for other
pub const S_IWOTH: i32 = 0o00_0002;
/// X for other
pub const S_IXOTH: i32 = 0o00_0001;

/// type of file mask
pub const S_IFMT: i32 = 0o17_0000;
/// named pipe (fifo)
pub const S_IFIFO: i32 = 0o01_0000;
/// character special
pub const S_IFCHR: i32 = 0o02_0000;
/// directory
pub const S_IFDIR: i32 = 0o04_0000;
/// block special
pub const S_IFBLK: i32 = 0o06_0000;
/// regular
pub const S_IFREG: i32 = 0o10_0000;
/// symbolic link
pub const S_IFLNK: i32 = 0o12_0000;
/// socket
pub const S_IFSOCK: i32 = 0o14_0000;
/// save swapped text even after use
pub const S_ISVTX: i32 = 0o00_1000;
/// whiteout
pub const S_IFWHT: i32 = 0o16_0000;

/// directory
#[must_use]
pub const fn S_ISDIR(m: i32) -> bool {
    (m & 0o17_0000) == 0o04_0000
}

/// char special
#[must_use]
pub const fn S_ISCHR(m: i32) -> bool {
    (m & 0o17_0000) == 0o02_0000
}

/// block special
#[must_use]
pub const fn S_ISBLK(m: i32) -> bool {
    (m & 0o17_0000) == 0o06_0000
}

/// regular file
#[must_use]
pub const fn S_ISREG(m: i32) -> bool {
    (m & 0o17_0000) == 0o10_0000
}

/// fifo or socket
#[must_use]
pub const fn S_ISFIFO(m: i32) -> bool {
    (m & 0o17_0000) == 0o01_0000
}

/// symbolic link
#[must_use]
pub const fn S_ISLNK(m: i32) -> bool {
    (m & 0o17_0000) == 0o12_0000
}

/// socket
#[must_use]
pub const fn S_ISSOCK(m: i32) -> bool {
    (m & 0o17_0000) == 0o14_0000
}

/// whiteout
#[must_use]
pub const fn S_ISWHT(m: i32) -> bool {
    (m & 0o17_0000) == 0o16_0000
}

/// 0777
pub const ACCESSPERMS: i32 = S_IRWXU | S_IRWXG | S_IRWXO;
/// 7777
pub const ALLPERMS: i32 = S_ISUID | S_ISGID | S_ISTXT | S_IRWXU | S_IRWXG | S_IRWXO;
/// 0666
pub const DEFFILEMODE: i32 = S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP | S_IROTH | S_IWOTH;

/// block size used in the stat struct
pub const S_BLKSIZE: usize = 512;

/// Definitions of flags stored in file flags word.
///
/// Super-user and owner changeable flags.
/// mask of owner changeable flags
pub const UF_SETTABLE: fflags_t = 0x0000_ffff;
/// do not dump file
pub const UF_NODUMP: fflags_t = 0x0000_0001;
/// file may not be changed
pub const UF_IMMUTABLE: fflags_t = 0x0000_0002;
/// writes to file may only append
pub const UF_APPEND: fflags_t = 0x0000_0004;
/// directory is opaque wrt. union
pub const UF_OPAQUE: fflags_t = 0x0000_0008;
/// file may not be removed or renamed
pub const UF_NOUNLINK: fflags_t = 0x0000_0010;

/// Windows system file bit
pub const UF_SYSTEM: fflags_t = 0x0000_0080;
/// sparse file
pub const UF_SPARSE: fflags_t = 0x0000_0100;
/// file is offline
pub const UF_OFFLINE: fflags_t = 0x0000_0200;
/// Windows reparse point file bit
pub const UF_REPARSE: fflags_t = 0x0000_0400;
/// file needs to be archived
pub const UF_ARCHIVE: fflags_t = 0x0000_0800;
/// Windows readonly file bit
pub const UF_READONLY: fflags_t = 0x0000_1000;
/// This is the same as the `MacOS X` definition of `UF_HIDDEN`.
/// file is hidden
pub const UF_HIDDEN: fflags_t = 0x0000_8000;

/// Super-user changeable flags.
/// mask of superuser changeable flags
pub const SF_SETTABLE: fflags_t = 0xffff_0000;
/// file is archived
pub const SF_ARCHIVED: fflags_t = 0x0001_0000;
/// file may not be changed
pub const SF_IMMUTABLE: fflags_t = 0x0002_0000;
/// writes to file may only append
pub const SF_APPEND: fflags_t = 0x0004_0000;
/// file may not be removed or renamed
pub const SF_NOUNLINK: fflags_t = 0x0010_0000;
/// snapshot inode
pub const SF_SNAPSHOT: fflags_t = 0x0020_0000;

pub const UTIME_NOW: i32 = -1;
pub const UTIME_OMIT: i32 = -2;
