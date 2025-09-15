// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/mount.h`

use crate::{c_char, gid_t, size_t, sockaddr_t, uid_t};

/// filesystem id type
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct fsid_t {
    pub val: [i32; 2],
}

/// File identifier.
///
/// These are unique per filesystem on a single machine.
///
/// Note that the offset of `fid_data` is 4 bytes, so care must be taken to avoid
/// undefined behavior accessing unaligned fields within an embedded struct.
pub const MAXFIDSZ: usize = 16;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct fid_t {
    /// length of data in bytes
    pub fid_len: u16,

    /// force longword alignment
    pub fid_data0: u16,

    /// data (variable length)
    pub fid_data: [u8; MAXFIDSZ],
}

/// filesystem statistics
/// length of type name including null
pub const MFSNAMELEN: usize = 16;
/// size of on/from name bufs
pub const MNAMELEN: usize = 1024;
/// current version number
pub const STATFS_VERSION: i32 = 0x2014_0518;

#[repr(C)]
#[derive(Debug)]
pub struct statfs_t {
    /// structure version number
    pub f_version: u32,

    /// type of filesystem
    pub f_type: u32,

    /// copy of mount exported flags
    pub f_flags: u64,

    /// filesystem fragment size
    pub f_bsize: u64,

    /// optimal transfer block size
    pub f_iosize: u64,

    /// total data blocks in filesystem
    pub f_blocks: u64,

    /// free blocks in filesystem
    pub f_bfree: u64,

    /// free blocks avail to non-superuser
    pub f_bavail: i64,

    /// total file nodes in filesystem
    pub f_files: u64,

    /// free nodes avail to non-superuser
    pub f_ffree: i64,

    /// count of sync writes since mount
    pub f_syncwrites: u64,

    /// count of async writes since mount
    pub f_asyncwrites: u64,

    /// count of sync reads since mount
    pub f_syncreads: u64,

    /// count of async reads since mount
    pub f_asyncreads: u64,

    /// unused spare
    pub f_spare: [u64; 10],

    /// maximum filename length
    pub f_namemax: u32,

    /// user that mounted the filesystem
    pub f_owner: uid_t,

    /// filesystem id
    pub f_fsid: fsid_t,

    /// spare string space
    pub f_charspare: [u8; 80],

    /// filesystem type name
    pub f_fstypename: [u8; MFSNAMELEN],

    /// mounted filesystem
    pub f_mntfromname: [u8; MNAMELEN],

    /// directory on which mounted
    pub f_mntonname: [u8; MNAMELEN],
}

impl Default for statfs_t {
    fn default() -> Self {
        Self {
            f_version: 0,
            f_type: 0,
            f_flags: 0,
            f_bsize: 0,
            f_iosize: 0,
            f_blocks: 0,
            f_bfree: 0,
            f_bavail: 0,
            f_files: 0,
            f_ffree: 0,
            f_syncwrites: 0,
            f_asyncwrites: 0,
            f_syncreads: 0,
            f_asyncreads: 0,
            f_spare: [0; 10],
            f_namemax: 0,
            f_owner: 0,
            f_fsid: fsid_t::default(),
            f_charspare: [0; 80],
            f_fstypename: [0; MFSNAMELEN],
            f_mntfromname: [0; MNAMELEN],
            f_mntonname: [0; MNAMELEN],
        }
    }
}

/// User specifiable flags, stored in `mnt_flag`.
/// read only filesystem
pub const MNT_RDONLY: u64 = 0x0000_0000_0000_0001;
/// fs written synchronously
pub const MNT_SYNCHRONOUS: u64 = 0x0000_0000_0000_0002;
/// can't exec from filesystem
pub const MNT_NOEXEC: u64 = 0x0000_0000_0000_0004;
/// don't honor setuid fs bits
pub const MNT_NOSUID: u64 = 0x0000_0000_0000_0008;
/// enable NFS version 4 ACLs
pub const MNT_NFS4ACLS: u64 = 0x0000_0000_0000_0010;
/// union with underlying fs
pub const MNT_UNION: u64 = 0x0000_0000_0000_0020;
/// fs written asynchronously
pub const MNT_ASYNC: u64 = 0x0000_0000_0000_0040;
/// special SUID dir handling
pub const MNT_SUIDDIR: u64 = 0x0000_0000_0010_0000;
/// using soft updates
pub const MNT_SOFTDEP: u64 = 0x0000_0000_0020_0000;
/// do not follow symlinks
pub const MNT_NOSYMFOLLOW: u64 = 0x0000_0000_0040_0000;
/// GEOM journal support enabled
pub const MNT_GJOURNAL: u64 = 0x0000_0000_0200_0000;
/// MAC support for objects
pub const MNT_MULTILABEL: u64 = 0x0000_0000_0400_0000;
/// ACL support enabled
pub const MNT_ACLS: u64 = 0x0000_0000_0800_0000;
/// dont update file access time
pub const MNT_NOATIME: u64 = 0x0000_0000_1000_0000;
/// disable cluster read
pub const MNT_NOCLUSTERR: u64 = 0x0000_0000_4000_0000;
/// disable cluster write
pub const MNT_NOCLUSTERW: u64 = 0x0000_0000_8000_0000;
/// using journaled soft updates
pub const MNT_SUJ: u64 = 0x0000_0001_0000_0000;
/// mounted by `automountd(8)`
pub const MNT_AUTOMOUNTED: u64 = 0x0000_0002_0000_0000;
/// filesys metadata untrusted
pub const MNT_UNTRUSTED: u64 = 0x0000_0008_0000_0000;

/// NFS export related mount flags.
/// exported read only
pub const MNT_EXRDONLY: u64 = 0x0000_0000_0000_0080;
/// filesystem is exported
pub const MNT_EXPORTED: u64 = 0x0000_0000_0000_0100;
/// exported to the world
pub const MNT_DEFEXPORTED: u64 = 0x0000_0000_0000_0200;
/// anon uid mapping for all
pub const MNT_EXPORTANON: u64 = 0x0000_0000_0000_0400;
/// exported with Kerberos
pub const MNT_EXKERB: u64 = 0x0000_0000_0000_0800;
/// public export (`WebNFS`)
pub const MNT_EXPUBLIC: u64 = 0x0000_0000_2000_0000;
/// require TLS
pub const MNT_EXTLS: u64 = 0x0000_0040_0000_0000;
/// require TLS with client cert
pub const MNT_EXTLSCERT: u64 = 0x0000_0080_0000_0000;
/// require TLS with user cert
pub const MNT_EXTLSCERTUSER: u64 = 0x0000_0100_0000_0000;

/// Flags set by internal operations, but visible to the user.
/// filesystem is stored locally
pub const MNT_LOCAL: u64 = 0x0000_0000_0000_1000;
/// quotas are enabled on fs
pub const MNT_QUOTA: u64 = 0x0000_0000_0000_2000;
/// identifies the root fs
pub const MNT_ROOTFS: u64 = 0x0000_0000_0000_4000;
/// mounted by a user
pub const MNT_USER: u64 = 0x0000_0000_0000_8000;
/// do not show entry in df
pub const MNT_IGNORE: u64 = 0x0000_0000_0080_0000;
/// filesystem is verified
pub const MNT_VERIFIED: u64 = 0x0000_0004_0000_0000;

/// Mask of flags that are visible to statfs().
pub const MNT_VISFLAGMASK: u64 = MNT_RDONLY
    | MNT_SYNCHRONOUS
    | MNT_NOEXEC
    | MNT_NOSUID
    | MNT_UNION
    | MNT_SUJ
    | MNT_ASYNC
    | MNT_EXRDONLY
    | MNT_EXPORTED
    | MNT_DEFEXPORTED
    | MNT_EXPORTANON
    | MNT_EXKERB
    | MNT_LOCAL
    | MNT_USER
    | MNT_QUOTA
    | MNT_ROOTFS
    | MNT_NOATIME
    | MNT_NOCLUSTERR
    | MNT_NOCLUSTERW
    | MNT_SUIDDIR
    | MNT_SOFTDEP
    | MNT_IGNORE
    | MNT_EXPUBLIC
    | MNT_NOSYMFOLLOW
    | MNT_GJOURNAL
    | MNT_MULTILABEL
    | MNT_ACLS
    | MNT_NFS4ACLS
    | MNT_AUTOMOUNTED
    | MNT_VERIFIED
    | MNT_UNTRUSTED;

/// Mask of flags that can be updated.
pub const MNT_UPDATEMASK: u64 = MNT_NOSUID
    | MNT_NOEXEC
    | MNT_SYNCHRONOUS
    | MNT_UNION
    | MNT_ASYNC
    | MNT_NOATIME
    | MNT_NOSYMFOLLOW
    | MNT_IGNORE
    | MNT_NOCLUSTERR
    | MNT_NOCLUSTERW
    | MNT_SUIDDIR
    | MNT_ACLS
    | MNT_USER
    | MNT_NFS4ACLS
    | MNT_AUTOMOUNTED
    | MNT_UNTRUSTED;

/// External filesystem command modifier flags.
/// Unmount can use the `MNT_FORCE` flag.
///
/// not real mount, just update
pub const MNT_UPDATE: u64 = 0x0000_0000_0001_0000;
/// delete export host lists
pub const MNT_DELEXPORT: u64 = 0x0000_0000_0002_0000;
/// reload filesystem data
pub const MNT_RELOAD: u64 = 0x0000_0000_0004_0000;
/// force unmount or readonly
pub const MNT_FORCE: u64 = 0x0000_0000_0008_0000;
/// snapshot the filesystem
pub const MNT_SNAPSHOT: u64 = 0x0000_0000_0100_0000;
/// check vnode use counts.
pub const MNT_NONBUSY: u64 = 0x0000_0000_0400_0000;
/// specify filesystem by ID.
pub const MNT_BYFSID: u64 = 0x0000_0000_0800_0000;
/// Do not cover a mount point
pub const MNT_NOCOVER: u64 = 0x0000_0010_0000_0000;
/// Only mount on empty dir
pub const MNT_EMPTYDIR: u64 = 0x0000_0020_0000_0000;
/// recursively unmount uppers
pub const MNT_RECURSE: u64 = 0x0000_1000_0000_0000;
/// unmount in async context
pub const MNT_DEFERRED: u64 = 0x0000_2000_0000_0000;
pub const MNT_CMDFLAGS: u64 = MNT_UPDATE
    | MNT_DELEXPORT
    | MNT_RELOAD
    | MNT_FORCE
    | MNT_SNAPSHOT
    | MNT_NONBUSY
    | MNT_BYFSID
    | MNT_NOCOVER
    | MNT_EMPTYDIR
    | MNT_RECURSE
    | MNT_DEFERRED;

/// Internal filesystem control flags stored in `mnt_kern_flag`.
///
/// `MNTK_UNMOUNT` locks the mount entry so that name lookup cannot
/// proceed past the mount point.  This keeps the subtree stable during
/// mounts and unmounts.  When non-forced unmount flushes all vnodes
/// from the mp queue, the `MNTK_UNMOUNT` flag prevents `insmntque()` from
/// queueing new vnodes.
///
/// `MNTK_UNMOUNTF` permits filesystems to detect a forced unmount while
/// `dounmount()` is still waiting to lock the mountpoint. This allows
/// the filesystem to cancel operations that might otherwise deadlock
/// with the unmount attempt (used by NFS).
/// forced unmount in progress
pub const MNTK_UNMOUNTF: i32 = 0x0000_0001;
/// filtered async flag
pub const MNTK_ASYNC: i32 = 0x0000_0002;
/// async disabled by softdep
pub const MNTK_SOFTDEP: i32 = 0x0000_0004;
/// don't do msync
pub const MNTK_NOMSYNC: i32 = 0x0000_0008;
/// lock draining is happening
pub const MNTK_DRAINING: i32 = 0x0000_0010;
/// refcount expiring is happening
pub const MNTK_REFEXPIRE: i32 = 0x0000_0020;
/// Allow shared locking for more ops
pub const MNTK_EXTENDED_SHARED: i32 = 0x0000_0040;
/// Allow shared locking for writes
pub const MNTK_SHARED_WRITES: i32 = 0x0000_0080;
/// Disallow page faults during reads and writes.
///
/// Filesystem shall properly handle i/o state on EFAULT.
pub const MNTK_NO_IOPF: i32 = 0x0000_0100;
/// pending recursive unmount
pub const MNTK_RECURSE: i32 = 0x0000_0200;
/// waiting to drain `MNTK_UPPER_PENDING`
pub const MNTK_UPPER_WAITER: i32 = 0x0000_0400;
pub const MNTK_LOOKUP_EXCL_DOTDOT: i32 = 0x0000_0800;
pub const MNTK_UNMAPPED_BUFS: i32 = 0x0000_2000;
/// FS uses the buffer cache.
pub const MNTK_USES_BCACHE: i32 = 0x0000_4000;
/// Keep use ref for text
pub const MNTK_TEXT_REFS: i32 = 0x0000_8000;
pub const MNTK_VMSETSIZE_BUG: i32 = 0x0001_0000;
/// A hack for `F_ISUNIONSTACK`
pub const MNTK_UNIONFS: i32 = 0x0002_0000;
/// fast path lookup is supported
pub const MNTK_FPLOOKUP: i32 = 0x0004_0000;
/// Suspended by all-fs suspension
pub const MNTK_SUSPEND_ALL: i32 = 0x0008_0000;
/// Waiting on unmount taskqueue
pub const MNTK_TASKQUEUE_WAITER: i32 = 0x0010_0000;
/// disable async
pub const MNTK_NOASYNC: i32 = 0x0080_0000;
/// unmount in progress
pub const MNTK_UNMOUNT: i32 = 0x0100_0000;
/// waiting for unmount to finish
pub const MNTK_MWAIT: i32 = 0x0200_0000;
/// request write suspension
pub const MNTK_SUSPEND: i32 = 0x0800_0000;
/// block secondary writes
pub const MNTK_SUSPEND2: i32 = 0x0400_0000;
/// write operations are suspended
pub const MNTK_SUSPENDED: i32 = 0x1000_0000;
/// auto disable cache for nullfs mounts over this fs
pub const MNTK_N_NOCACHE: i32 = 0x2000_0000;
/// FS supports shared lock lookups
pub const MNTK_LOOKUP_SHARED: i32 = 0x4000_0000;
/// Don't send KNOTEs from VOP hooks
#[allow(overflowing_literals)]
pub const MNTK_NOKNOTE: i32 = 0x8000_0000;

/// Flags for various system call interfaces.
///
/// waitfor flags to `vfs_sync()` and `getfsstat()`
/// synchronously wait for I/O to complete
pub const MNT_WAIT: i32 = 1;
/// start all I/O, but do not wait for it
pub const MNT_NOWAIT: i32 = 2;
/// push data not written by filesystem syncer
pub const MNT_LAZY: i32 = 3;
/// Suspend file system after sync
pub const MNT_SUSPEND: i32 = 4;

/// Generic file handle
#[repr(C)]
pub struct fhandle_t {
    /// Filesystem id of mount point
    pub fh_fsid: fsid_t,
    /// Filesys specific id
    pub fh_fid: fid_t,
}

/// Export arguments for local filesystem mount calls.
#[repr(C)]
pub struct export_args_t {
    /// export related flags
    pub ex_flags: u64,
    /// mapping for root uid
    pub ex_root: uid_t,
    /// mapping for anonymous user
    pub ex_uid: uid_t,
    pub ex_ngroups: i32,
    pub ex_groups: *mut gid_t,
    /// net address to which exported
    pub ex_addr: *mut sockaddr_t,
    /// and the net address length
    pub ex_addrlen: u8,
    /// mask of valid bits in saddr
    pub ex_mask: *mut sockaddr_t,
    /// and the smask length
    pub ex_masklen: u8,
    /// index file for WebNFS URLs
    pub ex_indexfile: *mut c_char,
    /// security flavor count
    pub ex_numsecflavors: i32,
    /// list of security flavors
    pub ex_secflavors: [i32; MAXSECFLAVORS],
}

pub const MAXSECFLAVORS: usize = 5;

//*
// * Structure holding information for a publicly exported filesystem
// * (WebNFS). Currently the specs allow just for one such filesystem.
// */
//struct nfs_public {
//	int		np_valid;	/* Do we hold valid information */
//	fhandle_t	np_handle;	/* Filehandle for pub fs (internal) */
//	struct mount	*np_mount;	/* Mountpoint of exported fs */
//	char		*np_index;	/* Index file */
//};
//
// Userland version of the struct vfsconf.
//#[repr(C)]
//pub struct xvfsconf_t {
//	struct	vfsops *vfc_vfsops;	/* filesystem operations vector */
//	char	vfc_name[MFSNAMELEN];	/* filesystem type name */
//	int	vfc_typenum;		/* historic filesystem type number */
//	int	vfc_refcount;		/* number mounted of this type */
//	int	vfc_flags;		/* permanent flags */
//	struct	vfsconf *vfc_next;	/* next in list */
//}

/*
 * NB: these flags refer to IMPLEMENTATION properties, not properties of
 * any actual mounts; i.e., it does not make sense to change the flags.
 */
/// statically compiled into kernel
pub const VFCF_STATIC: i32 = 0x00010000;
/// may get data over the network
pub const VFCF_NETWORK: i32 = 0x00020000;
/// writes are not implemented
pub const VFCF_READONLY: i32 = 0x00040000;
/// data does not represent real files
pub const VFCF_SYNTHETIC: i32 = 0x00080000;
/// aliases some other mounted FS
pub const VFCF_LOOPBACK: i32 = 0x00100000;
/// stores file names as Unicode
pub const VFCF_UNICODE: i32 = 0x00200000;
/// can be mounted from within a jail
pub const VFCF_JAIL: i32 = 0x00400000;
/// supports delegated administration
pub const VFCF_DELEGADMIN: i32 = 0x00800000;
/// Stop at Boundary: defer stop requests to kernel->user (AST) transition
pub const VFCF_SBDRY: i32 = 0x01000000;
/// allow mounting files
pub const VFCF_FILEMOUNT: i32 = 0x02000000;

pub type fsctlop_t = u32;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct vfsidctl_t {
    /// should be VFSIDCTL_VERS1 (below)
    pub vc_vers: i32,

    /// fsid to operate on
    pub vc_fsid: fsid_t,

    /// type of fs 'nfs' or '*'
    pub vc_fstypename: [c_char; MFSNAMELEN],

    /// operation VFS_CTL_* (below)
    pub vc_op: fsctlop_t,

    /// pointer to data structure
    pub vc_ptr: usize,

    /// sizeof said structure
    pub vc_len: size_t,

    /// spare (must be zero)
    pub vc_spare: [i32; 12],
}

/// vfsidctl API version.
pub const VFS_CTL_VERS1: i32 = 0x01;

/*
 * New style VFS sysctls, do not reuse/conflict with the namespace for
 * private sysctls.
 * All "global" sysctl ops have the 33rd bit set:
 * 0x...1....
 * Private sysctl ops should have the 33rd bit unset.
 */
/// anything wrong? (vfsquery)
pub const VFS_CTL_QUERY: i32 = 0x00010001;
/// set timeout for vfs notification
pub const VFS_CTL_TIMEO: i32 = 0x00010002;
/// disable file locking
pub const VFS_CTL_NOLOCKS: i32 = 0x00010003;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct vfsquery_t {
    pub vq_flags: u32,
    pub vq_spare: [u32; 31],
}

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
