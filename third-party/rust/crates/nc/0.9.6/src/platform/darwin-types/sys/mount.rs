// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/mount.h`
//!
//! file system statistics

#![allow(overflowing_literals)]

use crate::{c_char, fsid_t, uid_t, MAXPATHLEN};

/// length of fs type name, not inc. null
pub const MFSNAMELEN: usize = 15;
/// length of fs type name including null
pub const MFSTYPENAMELEN: usize = 16;

/// length of buffer for returned name
pub const MNAMELEN: usize = MAXPATHLEN;

/// Data volume of root volume group
pub const MNT_EXT_ROOT_DATA_VOL: i32 = 0x0000_0001;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct statfs64_t {
    /// fundamental file system block size
    pub f_bsize: u32,

    /// optimal transfer block size
    pub f_iosize: i32,

    /// total data blocks in file system
    pub f_blocks: u64,

    /// free blocks in fs
    pub f_bfree: u64,

    /// free blocks avail to non-superuser
    pub f_bavail: u64,

    /// total file nodes in file system
    pub f_files: u64,

    /// free file nodes in fs
    pub f_ffree: u64,

    /// file system id
    pub f_fsid: fsid_t,

    /// user that mounted the filesystem
    pub f_owner: uid_t,

    /// type of filesystem
    pub f_type: u32,

    /// copy of mount exported flags
    pub f_flags: u32,

    /// fs sub-type (flavor)
    pub f_fssubtype: u32,

    /// fs type name
    pub f_fstypename: [c_char; MFSTYPENAMELEN],
    /// directory on which mounted
    pub f_mntonname: [c_char; MAXPATHLEN],

    /// mounted filesystem
    pub f_mntfromname: [c_char; MAXPATHLEN],

    /// extended flags
    pub f_flags_ext: u32,

    /// For future use
    f_reserved: [u32; 7],
}

pub type statfs_t = statfs64_t;

/// User specifiable flags.
///
/// Unmount uses MNT_FORCE flag.
///
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
/// file system supports content protection
pub const MNT_CPROTECT: i32 = 0x0000_0080;

/// NFS export related mount flags.
///
/// file system is exported
pub const MNT_EXPORTED: i32 = 0x0000_0100;

/// Denotes storage which can be removed from the system by the user.
pub const MNT_REMOVABLE: i32 = 0x0000_0200;

/// MAC labeled / "quarantined" flag
///
/// file system is quarantined
pub const MNT_QUARANTINE: i32 = 0x0000_0400;

/// Flags set by internal operations.
///
/// filesystem is stored locally
pub const MNT_LOCAL: i32 = 0x0000_1000;
/// quotas are enabled on filesystem
pub const MNT_QUOTA: i32 = 0x0000_2000;
/// identifies the root filesystem
pub const MNT_ROOTFS: i32 = 0x0000_4000;
/// FS supports volfs (deprecated flag in Mac OS X 10.5)
pub const MNT_DOVOLFS: i32 = 0x0000_8000;

/// file system is not appropriate path to user data
pub const MNT_DONTBROWSE: i32 = 0x0010_0000;
/// VFS will ignore ownership information on filesystem objects
pub const MNT_IGNORE_OWNERSHIP: i32 = 0x0020_0000;
/// filesystem was mounted by automounter
pub const MNT_AUTOMOUNTED: i32 = 0x0040_0000;
/// filesystem is journaled
pub const MNT_JOURNALED: i32 = 0x0080_0000;
/// Don't allow user extended attributes
pub const MNT_NOUSERXATTR: i32 = 0x0100_0000;
/// filesystem should defer writes
pub const MNT_DEFWRITE: i32 = 0x0200_0000;
/// MAC support for individual labels
pub const MNT_MULTILABEL: i32 = 0x0400_0000;
/// disable update of file access time
pub const MNT_NOATIME: i32 = 0x1000_0000;
/// The mount is a snapshot
pub const MNT_SNAPSHOT: i32 = 0x4000_0000;
/// enable strict update of file access time
pub const MNT_STRICTATIME: i32 = 0x8000_0000;

/// backwards compatibility only
pub const MNT_UNKNOWNPERMISSIONS: i32 = MNT_IGNORE_OWNERSHIP;

pub const MNT_VISFLAGMASK: i32 = MNT_RDONLY
    | MNT_SYNCHRONOUS
    | MNT_NOEXEC
    | MNT_NOSUID
    | MNT_NODEV
    | MNT_UNION
    | MNT_ASYNC
    | MNT_EXPORTED
    | MNT_QUARANTINE
    | MNT_LOCAL
    | MNT_QUOTA
    | MNT_REMOVABLE
    | MNT_ROOTFS
    | MNT_DOVOLFS
    | MNT_DONTBROWSE
    | MNT_IGNORE_OWNERSHIP
    | MNT_AUTOMOUNTED
    | MNT_JOURNALED
    | MNT_NOUSERXATTR
    | MNT_DEFWRITE
    | MNT_MULTILABEL
    | MNT_NOATIME
    | MNT_STRICTATIME
    | MNT_SNAPSHOT
    | MNT_CPROTECT;

/// External filesystem command modifier flags.
/// Unmount can use the MNT_FORCE flag.
///
/// not a real mount, just an update
pub const MNT_UPDATE: i32 = 0x0001_0000;
/// don't block unmount if not responding
pub const MNT_NOBLOCK: i32 = 0x0002_0000;
/// reload filesystem data
pub const MNT_RELOAD: i32 = 0x0004_0000;
/// force unmount or readonly change
pub const MNT_FORCE: i32 = 0x0008_0000;
pub const MNT_CMDFLAGS: i32 = MNT_UPDATE | MNT_NOBLOCK | MNT_RELOAD | MNT_FORCE;

/// Sysctl CTL_VFS definitions.
///
/// Second level identifier specifies which filesystem. Second level
/// identifier VFS_GENERIC returns information about all filesystems.
///
/// generic filesystem information
pub const VFS_GENERIC: i32 = 0;
/// int: total num of vfs mount/unmount operations
pub const VFS_NUMMNTOPS: i32 = 1;

/// Third level identifiers for VFS_GENERIC are given below; third
/// level identifiers for specific filesystems are given in their
/// mount specific header files.
///
/// int: highest defined filesystem type
pub const VFS_MAXTYPENUM: i32 = 1;
/// struct: vfsconf for filesystem given as next argument
pub const VFS_CONF: i32 = 2;

/// Flags for various system call interfaces.
///
/// waitfor flags to vfs_sync() and getfsstat()
///
/// synchronized I/O file integrity completion
pub const MNT_WAIT: i32 = 1;
/// start all I/O, but do not wait for it
pub const MNT_NOWAIT: i32 = 2;
/// synchronized I/O data integrity completion
pub const MNT_DWAIT: i32 = 4;

/// vfsidctl API version.
pub const VFS_CTL_VERS1: i32 = 0x01;

/// New style VFS sysctls, do not reuse/conflict with the namespace for
/// private sysctls.
///
/// old legacy statfs
pub const VFS_CTL_OSTATFS: i32 = 0x0001_0001;
/// unmount
pub const VFS_CTL_UMOUNT: i32 = 0x0001_0002;
/// anything wrong? (vfsquery)
pub const VFS_CTL_QUERY: i32 = 0x0001_0003;
/// reconnect to new address
pub const VFS_CTL_NEWADDR: i32 = 0x0001_0004;
/// set timeout for vfs notification
pub const VFS_CTL_TIMEO: i32 = 0x0001_0005;
/// disable file locking
pub const VFS_CTL_NOLOCKS: i32 = 0x0001_0006;
/// get server address
pub const VFS_CTL_SADDR: i32 = 0x0001_0007;
/// server disconnected
pub const VFS_CTL_DISC: i32 = 0x0001_0008;
/// information about fs server
pub const VFS_CTL_SERVERINFO: i32 = 0x0001_0009;
/// netfs mount status
pub const VFS_CTL_NSTATUS: i32 = 0x0001_000A;
/// statfs64
pub const VFS_CTL_STATFS64: i32 = 0x0001_000B;

/// vfsquery flags
/// server down
pub const VQ_NOTRESP: i32 = 0x0001;
/// server bad auth
pub const VQ_NEEDAUTH: i32 = 0x0002;
/// we're low on space
pub const VQ_LOWDISK: i32 = 0x0004;
/// new filesystem arrived
pub const VQ_MOUNT: i32 = 0x0008;
/// filesystem has left
pub const VQ_UNMOUNT: i32 = 0x0010;
/// filesystem is dead, needs force unmount
pub const VQ_DEAD: i32 = 0x0020;
/// filesystem needs assistance from external program
pub const VQ_ASSIST: i32 = 0x0040;
/// server lockd down
pub const VQ_NOTRESPLOCK: i32 = 0x0080;
/// filesystem information has changed
pub const VQ_UPDATE: i32 = 0x0100;
/// file system has *very* little disk space left
pub const VQ_VERYLOWDISK: i32 = 0x0200;
/// a sync just happened (not set by kernel starting Mac OS X 10.9)
pub const VQ_SYNCEVENT: i32 = 0x0400;
/// server issued notification/warning
pub const VQ_SERVEREVENT: i32 = 0x0800;
/// a user quota has been hit
pub const VQ_QUOTA: i32 = 0x1000;
/// Above lowdisk and below desired disk space
pub const VQ_NEARLOWDISK: i32 = 0x2000;
/// the desired disk space
pub const VQ_DESIRED_DISK: i32 = 0x4000;
/// free disk space has significantly changed
pub const VQ_FREE_SPACE_CHANGE: i32 = 0x8000;
/// placeholder
pub const VQ_FLAG10000: i32 = 0x10000;

/// Generic file handle
pub const NFSV4_MAX_FH_SIZE: usize = 128;
pub const NFSV3_MAX_FH_SIZE: usize = 64;
pub const NFSV2_MAX_FH_SIZE: usize = 32;
pub const NFS_MAX_FH_SIZE: usize = NFSV4_MAX_FH_SIZE;

pub struct fhandle_t {
    /// length of file handle
    pub fh_len: u32,

    /// file handle value
    pub fh_data: [u8; NFS_MAX_FH_SIZE],
}
