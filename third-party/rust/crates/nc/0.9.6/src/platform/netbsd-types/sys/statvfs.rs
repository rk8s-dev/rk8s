// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/statvfs.h`

use crate::{
    c_char, fsblkcnt_t, fsfilcnt_t, fsid_t, uid_t, MNT_ASYNC, MNT_DEFEXPORTED, MNT_EXKERB,
    MNT_EXNORESPORT, MNT_EXPORTANON, MNT_EXPORTED, MNT_EXPUBLIC, MNT_EXRDONLY, MNT_EXTATTR,
    MNT_IGNORE, MNT_LOCAL, MNT_LOG, MNT_NOATIME, MNT_NOCOREDUMP, MNT_NODEV, MNT_NODEVMTIME,
    MNT_NOEXEC, MNT_NOSUID, MNT_NOWAIT, MNT_QUOTA, MNT_RDONLY, MNT_RELATIME, MNT_ROOTFS,
    MNT_SOFTDEP, MNT_SYMPERM, MNT_SYNCHRONOUS, MNT_UNION, MNT_WAIT,
};

pub const _VFS_NAMELEN: usize = 32;
pub const _VFS_MNAMELEN: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct statvfs_t {
    /// copy of mount exported flags
    pub f_flag: usize,
    /// file system block size
    pub f_bsize: usize,
    /// fundamental file system block size
    pub f_frsize: usize,
    /// optimal file system block size
    pub f_iosize: usize,

    // The following are in units of f_frsize
    /// number of blocks in file system
    pub f_blocks: fsblkcnt_t,
    /// free blocks avail in file system
    pub f_bfree: fsblkcnt_t,
    /// free blocks avail to non-root
    pub f_bavail: fsblkcnt_t,
    /// blocks reserved for root
    pub f_bresvd: fsblkcnt_t,

    /// total file nodes in file system
    pub f_files: fsfilcnt_t,
    /// free file nodes in file system
    pub f_ffree: fsfilcnt_t,
    /// free file nodes avail to non-root
    pub f_favail: fsfilcnt_t,
    /// file nodes reserved for root
    pub f_fresvd: fsfilcnt_t,

    /// count of sync reads since mount
    pub f_syncreads: u64,
    /// count of sync writes since mount
    pub f_syncwrites: u64,

    /// count of async reads since mount
    pub f_asyncreads: u64,
    /// count of async writes since mount
    pub f_asyncwrites: u64,

    /// NetBSD compatible fsid
    pub f_fsidx: fsid_t,
    /// Posix compatible fsid
    pub f_fsid: usize,
    /// maximum filename length
    pub f_namemax: usize,
    /// user that mounted the file system
    pub f_owner: uid_t,

    // spare space
    f_spare: [u32; 4],

    /// fs type name
    pub f_fstypename: [c_char; _VFS_NAMELEN],
    /// directory on which mounted
    pub f_mntonname: [c_char; _VFS_MNAMELEN],
    /// mounted file system
    pub f_mntfromname: [c_char; _VFS_MNAMELEN],
}

pub const VFS_NAMELEN: usize = _VFS_NAMELEN;
pub const VFS_MNAMELEN: usize = _VFS_MNAMELEN;

pub const ST_RDONLY: i32 = MNT_RDONLY;
pub const ST_SYNCHRONOUS: i32 = MNT_SYNCHRONOUS;
pub const ST_NOEXEC: i32 = MNT_NOEXEC;
pub const ST_NOSUID: i32 = MNT_NOSUID;
pub const ST_NODEV: i32 = MNT_NODEV;
pub const ST_UNION: i32 = MNT_UNION;
pub const ST_ASYNC: i32 = MNT_ASYNC;
pub const ST_NOCOREDUMP: i32 = MNT_NOCOREDUMP;
pub const ST_RELATIME: i32 = MNT_RELATIME;
pub const ST_IGNORE: i32 = MNT_IGNORE;
pub const ST_NOATIME: i32 = MNT_NOATIME;
pub const ST_SYMPERM: i32 = MNT_SYMPERM;
pub const ST_NODEVMTIME: i32 = MNT_NODEVMTIME;
pub const ST_SOFTDEP: i32 = MNT_SOFTDEP;
pub const ST_LOG: i32 = MNT_LOG;
pub const ST_EXTATTR: i32 = MNT_EXTATTR;

pub const ST_EXRDONLY: i32 = MNT_EXRDONLY;
pub const ST_EXPORTED: i32 = MNT_EXPORTED;
pub const ST_DEFEXPORTED: i32 = MNT_DEFEXPORTED;
pub const ST_EXPORTANON: i32 = MNT_EXPORTANON;
pub const ST_EXKERB: i32 = MNT_EXKERB;
pub const ST_EXNORESPORT: i32 = MNT_EXNORESPORT;
pub const ST_EXPUBLIC: i32 = MNT_EXPUBLIC;

pub const ST_LOCAL: i32 = MNT_LOCAL;
pub const ST_QUOTA: i32 = MNT_QUOTA;
pub const ST_ROOTFS: i32 = MNT_ROOTFS;

pub const ST_WAIT: i32 = MNT_WAIT;
pub const ST_NOWAIT: i32 = MNT_NOWAIT;
