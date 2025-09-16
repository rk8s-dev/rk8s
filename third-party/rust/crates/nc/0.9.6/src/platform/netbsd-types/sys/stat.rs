// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/stat.h`

#![allow(clippy::module_name_repetitions)]
#![allow(overflowing_literals)]

use crate::{blkcnt_t, blksize_t, dev_t, gid_t, ino_t, mode_t, nlink_t, off_t, timespec_t, uid_t};

#[repr(C)]
pub struct stat_t {
    /// inode's device
    pub st_dev: dev_t,
    /// inode protection mode
    pub st_mode: mode_t,
    /// inode's number
    pub st_ino: ino_t,
    /// number of hard links
    pub st_nlink: nlink_t,
    /// user ID of the file's owner
    pub st_uid: uid_t,
    /// group ID of the file's group
    pub st_gid: gid_t,
    /// device type
    pub st_rdev: dev_t,
    /// time of last access
    pub st_atim: timespec_t,
    /// time of last data modification
    pub st_mtim: timespec_t,
    /// time of last file status change
    pub st_ctim: timespec_t,
    /// time of creation
    pub st_birthtim: timespec_t,
    /// file size, in bytes
    pub st_size: off_t,
    /// blocks allocated for file
    pub st_blocks: blkcnt_t,
    /// optimal blocksize for I/O
    pub st_blksize: blksize_t,
    /// user defined flags for file
    pub st_flags: u32,
    /// file generation number
    pub st_gen: u32,
    pub st_spare: [u32; 2],
}

/// set user id on execution
pub const S_ISUID: i32 = 0o004_000;
/// set group id on execution
pub const S_ISGID: i32 = 0o002_000;
/// sticky bit
pub const S_ISTXT: i32 = 0o001_000;

/// RWX mask for owner
pub const S_IRWXU: i32 = 0o000_700;
/// R for owner
pub const S_IRUSR: i32 = 0o000_400;
/// W for owner
pub const S_IWUSR: i32 = 0o000_200;
/// X for owner
pub const S_IXUSR: i32 = 0o000_100;

pub const S_IREAD: i32 = S_IRUSR;
pub const S_IWRITE: i32 = S_IWUSR;
pub const S_IEXEC: i32 = S_IXUSR;

/// RWX mask for group
pub const S_IRWXG: i32 = 0o000_070;
/// R for group
pub const S_IRGRP: i32 = 0o000_040;
/// W for group
pub const S_IWGRP: i32 = 0o000_020;
/// X for group
pub const S_IXGRP: i32 = 0o000_010;

/// RWX mask for other
pub const S_IRWXO: i32 = 0o000_007;
/// R for other
pub const S_IROTH: i32 = 0o000_004;
/// W for other
pub const S_IWOTH: i32 = 0o000_002;
/// X for other
pub const S_IXOTH: i32 = 0o000_001;

/// type of file mask
pub const _S_IFMT: i32 = 0o170_000;
/// named pipe (fifo)
pub const _S_IFIFO: i32 = 0o010_000;
/// character special
pub const _S_IFCHR: i32 = 0o020_000;
/// directory
pub const _S_IFDIR: i32 = 0o040_000;
/// block special
pub const _S_IFBLK: i32 = 0o060_000;
/// regular
pub const _S_IFREG: i32 = 0o100_000;
/// symbolic link
pub const _S_IFLNK: i32 = 0o120_000;
/// save swapped text even after use
pub const _S_ISVTX: i32 = 0o001_000;
/// socket
pub const _S_IFSOCK: i32 = 0o140_000;
/// whiteout
pub const _S_IFWHT: i32 = 0o160_000;
/// Archive state 1, ls -l shows 'a'
pub const _S_ARCH1: i32 = 0o200_000;
/// Archive state 2, ls -l shows 'A'
pub const _S_ARCH2: i32 = 0o400_000;

pub const S_IFMT: i32 = _S_IFMT;
pub const S_IFIFO: i32 = _S_IFIFO;
pub const S_IFCHR: i32 = _S_IFCHR;
pub const S_IFDIR: i32 = _S_IFDIR;
pub const S_IFBLK: i32 = _S_IFBLK;
pub const S_IFREG: i32 = _S_IFREG;
pub const S_IFLNK: i32 = _S_IFLNK;
pub const S_ISVTX: i32 = _S_ISVTX;

pub const S_IFSOCK: i32 = _S_IFSOCK;
pub const S_IFWHT: i32 = _S_IFWHT;

pub const S_ARCH1: i32 = _S_ARCH1;
pub const S_ARCH2: i32 = _S_ARCH2;

/// directory
#[must_use]
pub const fn S_ISDIR(m: i32) -> bool {
    (m & _S_IFMT) == _S_IFDIR
}

/// char special
#[must_use]
pub const fn S_ISCHR(m: i32) -> bool {
    (m & _S_IFMT) == _S_IFCHR
}

/// block special
#[must_use]
pub const fn S_ISBLK(m: i32) -> bool {
    (m & _S_IFMT) == _S_IFBLK
}

/// regular file
#[must_use]
pub const fn S_ISREG(m: i32) -> bool {
    (m & _S_IFMT) == _S_IFREG
}

/// fifo
#[must_use]
pub const fn S_ISFIFO(m: i32) -> bool {
    (m & _S_IFMT) == _S_IFIFO
}

/// symbolic link
#[must_use]
pub const fn S_ISLNK(m: i32) -> bool {
    (m & _S_IFMT) == _S_IFLNK
}

/// socket
#[must_use]
pub const fn S_ISSOCK(m: i32) -> bool {
    (m & _S_IFMT) == _S_IFSOCK
}

/// whiteout
#[must_use]
pub const fn S_ISWHT(m: i32) -> bool {
    (m & _S_IFMT) == _S_IFWHT
}

/// 0777
pub const ACCESSPERMS: i32 = S_IRWXU | S_IRWXG | S_IRWXO;
/// 7777
pub const ALLPERMS: i32 = S_ISUID | S_ISGID | S_ISTXT | S_IRWXU | S_IRWXG | S_IRWXO;
/* 0666 */
//pub const: i32 = 	DEFFILEMODE	(S_IRUSR|S_IWUSR|S_IRGRP|S_IWGRP|S_IROTH|S_IWOTH)

/// block size used in the stat struct
pub const S_BLKSIZE: i32 = 512;

/// Definitions of flags stored in file flags word.
///
/// Super-user and owner changeable flags.
///
/// mask of owner changeable flags
pub const UF_SETTABLE: i32 = 0x0000_ffff;
/// do not dump file
pub const UF_NODUMP: i32 = 0x0000_0001;
/// file may not be changed
pub const UF_IMMUTABLE: i32 = 0x0000_0002;
/// writes to file may only append
pub const UF_APPEND: i32 = 0x0000_0004;
/// directory is opaque wrt. union
pub const UF_OPAQUE: i32 = 0x0000_0008;
// UF_NOUNLINK	0x00000010	   [NOT IMPLEMENTED]

/// Super-user changeable flags.
/// mask of superuser changeable flags
pub const SF_SETTABLE: i32 = 0xffff_0000;
/// file is archived
pub const SF_ARCHIVED: i32 = 0x0001_0000;
/// file may not be changed
pub const SF_IMMUTABLE: i32 = 0x0002_0000;
/// writes to file may only append
pub const SF_APPEND: i32 = 0x0004_0000;
// SF_NOUNLINK	0x00100000	   [NOT IMPLEMENTED]
/// snapshot inode
pub const SF_SNAPSHOT: i32 = 0x0020_0000;
/// WAPBL log file inode
pub const SF_LOG: i32 = 0x0040_0000;
/// snapshot is invalid
pub const SF_SNAPINVAL: i32 = 0x0080_0000;

/// Shorthand abbreviations of above.
pub const OPAQUE: i32 = UF_OPAQUE;
pub const APPEND: i32 = UF_APPEND | SF_APPEND;
pub const IMMUTABLE: i32 = UF_IMMUTABLE | SF_IMMUTABLE;

/// Special values for utimensat and futimens
pub const UTIME_NOW: i32 = (1 << 30) - 1;
pub const UTIME_OMIT: i32 = (1 << 30) - 2;
