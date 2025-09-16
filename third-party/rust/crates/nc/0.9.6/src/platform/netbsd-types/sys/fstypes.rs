// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/fstypes.h`

use crate::c_char;

/// file system id type
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct fsid_t {
    pub __fsid_val: [i32; 2],
}

/// File identifier.
///
/// These are unique per filesystem on a single machine.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct fid_t {
    /// length of data in bytes
    pub fid_len: u16,

    /// compat: historic align
    pub fid_reserved: u16,

    /// data (variable length)
    pub fid_data: [c_char; 0],
}

/// Generic file handle
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct fhandle_t {
    /// File system id of mount point
    pub fh_fsid: fsid_t,

    /// File sys specific id
    pub fh_fid: fid_t,
}

/// Mount flags.
///
/// XXX BEWARE: these are not in numerical order!
///
/// Unmount uses MNT_FORCE flag.
///
/// Note that all mount flags are listed here.  if you need to add one, take
/// one of the __MNT_UNUSED flags.
pub const __MNT_UNUSED1: i32 = 0x0020_0000;

/// read only filesystem
pub const MNT_RDONLY: i32 = 0x0000_0001;
/// file system written synchronously
pub const MNT_SYNCHRONOUS: i32 = 0x0000_0002;
/// can't exec from filesystem
pub const MNT_NOEXEC: i32 = 0x0000_0004;
/// don't honor setuid bits on fs
pub const MNT_NOSUID: i32 = 0x0000_0008;
/// don't interpret special files
pub const MNT_NODEV: i32 = 0x0000_0010;
/// union with underlying filesystem
pub const MNT_UNION: i32 = 0x0000_0020;
/// file system written asynchronously
pub const MNT_ASYNC: i32 = 0x0000_0040;
/// don't write core dumps to this FS
pub const MNT_NOCOREDUMP: i32 = 0x0000_8000;
/// only update access time if mod/ch
pub const MNT_RELATIME: i32 = 0x0002_0000;
/// don't show entry in df
pub const MNT_IGNORE: i32 = 0x0010_0000;
/// use DISCARD/TRIM if supported
pub const MNT_DISCARD: i32 = 0x0080_0000;
/// enable extended attributes
pub const MNT_EXTATTR: i32 = 0x0100_0000;
/// Use logging
pub const MNT_LOG: i32 = 0x0200_0000;
/// Never update access times in fs
pub const MNT_NOATIME: i32 = 0x0400_0000;
/// mounted by automountd(8)
pub const MNT_AUTOMOUNTED: i32 = 0x1000_0000;
/// recognize symlink permission
pub const MNT_SYMPERM: i32 = 0x2000_0000;
/// Never update mod times for devs
pub const MNT_NODEVMTIME: i32 = 0x4000_0000;
/// Use soft dependencies
#[allow(overflowing_literals)]
pub const MNT_SOFTDEP: i32 = 0x8000_0000;

pub const MNT_BASIC_FLAGS: i32 = MNT_ASYNC
    | MNT_AUTOMOUNTED
    | MNT_DISCARD
    | MNT_EXTATTR
    | MNT_LOG
    | MNT_NOATIME
    | MNT_NOCOREDUMP
    | MNT_NODEV
    | MNT_NODEVMTIME
    | MNT_NOEXEC
    | MNT_NOSUID
    | MNT_RDONLY
    | MNT_RELATIME
    | MNT_SOFTDEP
    | MNT_SYMPERM
    | MNT_SYNCHRONOUS
    | MNT_UNION;

// exported mount flags.
/// exported read only
pub const MNT_EXRDONLY: i32 = 0x0000_0080;
/// file system is exported
pub const MNT_EXPORTED: i32 = 0x0000_0100;
/// exported to the world
pub const MNT_DEFEXPORTED: i32 = 0x0000_0200;
/// use anon uid mapping for everyone
pub const MNT_EXPORTANON: i32 = 0x0000_0400;
/// exported with Kerberos uid mapping
pub const MNT_EXKERB: i32 = 0x0000_0800;
/// don't enforce reserved ports (NFS)
pub const MNT_EXNORESPORT: i32 = 0x0800_0000;
/// public export (WebNFS)
pub const MNT_EXPUBLIC: i32 = 0x1000_0000;

// Flags set by internal operations.
/// filesystem is stored locally
pub const MNT_LOCAL: i32 = 0x0000_1000;
/// quotas are enabled on filesystem
pub const MNT_QUOTA: i32 = 0x0000_2000;
/// identifies the root filesystem
pub const MNT_ROOTFS: i32 = 0x0000_4000;

/// Mask of flags that are visible to statvfs()
pub const MNT_VISFLAGMASK: i32 = MNT_RDONLY
    | MNT_SYNCHRONOUS
    | MNT_NOEXEC
    | MNT_NOSUID
    | MNT_NODEV
    | MNT_UNION
    | MNT_ASYNC
    | MNT_NOCOREDUMP
    | MNT_IGNORE
    | MNT_DISCARD
    | MNT_NOATIME
    | MNT_SYMPERM
    | MNT_NODEVMTIME
    | MNT_SOFTDEP
    | MNT_EXRDONLY
    | MNT_EXPORTED
    | MNT_DEFEXPORTED
    | MNT_EXPORTANON
    | MNT_EXKERB
    | MNT_EXNORESPORT
    | MNT_EXPUBLIC
    | MNT_LOCAL
    | MNT_QUOTA
    | MNT_ROOTFS
    | MNT_LOG
    | MNT_EXTATTR
    | MNT_AUTOMOUNTED;

// External filesystem control flags.
/// not a real mount, just an update
pub const MNT_UPDATE: i32 = 0x0001_0000;
/// reload filesystem data
pub const MNT_RELOAD: i32 = 0x0004_0000;
/// force unmount or readonly change
pub const MNT_FORCE: i32 = 0x0008_0000;
/// retrieve file system specific args
pub const MNT_GETARGS: i32 = 0x0040_0000;

pub const MNT_OP_FLAGS: i32 = MNT_UPDATE | MNT_RELOAD | MNT_FORCE | MNT_GETARGS;

/// Internal filesystem control flags.
///
/// These are set in struct mount mnt_iflag.
///
/// IMNT_UNMOUNT locks the mount entry so that name lookup cannot proceed
/// past the mount point.  This keeps the subtree stable during mounts
/// and unmounts.
/// filesystem is gone..
pub const IMNT_GONE: i32 = 0x0000_0001;
/// unmount in progress
pub const IMNT_UNMOUNT: i32 = 0x0000_0002;
/// upgrade to read/write requested
pub const IMNT_WANTRDWR: i32 = 0x0000_0004;
/// upgrade to readonly requested
pub const IMNT_WANTRDONLY: i32 = 0x0000_0008;
/// returns d_type fields
pub const IMNT_DTYPE: i32 = 0x0000_0040;
/// file system code MP safe
pub const IMNT_MPSAFE: i32 = 0x0000_0100;
/// can downgrade fs to from rw to r/o
pub const IMNT_CAN_RWTORO: i32 = 0x0000_0200;
/// on syncer worklist
pub const IMNT_ONWORKLIST: i32 = 0x0000_0400;

// Flags for various system call interfaces.
//
// waitfor flags to vfs_sync() and getvfsstat()
/// synchronously wait for I/O to complete
pub const MNT_WAIT: i32 = 1;
/// start all I/O, but do not wait for it
pub const MNT_NOWAIT: i32 = 2;
/// push data not written by filesystem syncer
pub const MNT_LAZY: i32 = 3;
