// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/mount.h`

#![allow(clippy::module_name_repetitions)]

use crate::O_CLOEXEC;

/// These are the fs-independent mount-flags: up to 32 flags are supported
///
/// Usage of these is restricted within the kernel to core mount(2) code and
/// callers of `sys_mount()` only.  Filesystems should be using the `SB_*`
/// equivalent instead.
///
/// Mount read-only
pub const MS_RDONLY: u32 = 1;
/// Ignore suid and sgid bits
pub const MS_NOSUID: u32 = 2;
/// Disallow access to device special files
pub const MS_NODEV: u32 = 4;
/// Disallow program execution
pub const MS_NOEXEC: u32 = 8;
/// Writes are synced at once
pub const MS_SYNCHRONOUS: u32 = 16;
/// Alter flags of a mounted FS
pub const MS_REMOUNT: u32 = 32;
/// Allow mandatory locks on an FS
pub const MS_MANDLOCK: u32 = 64;
/// Directory modifications are synchronous
pub const MS_DIRSYNC: u32 = 128;
/// Do not update access times.
pub const MS_NOATIME: u32 = 1024;
/// Do not update directory access times
pub const MS_NODIRATIME: u32 = 2048;
pub const MS_BIND: u32 = 4096;
pub const MS_MOVE: u32 = 8192;
pub const MS_REC: u32 = 16384;
/// `MS_VERBOSE` is deprecated.
pub const MS_VERBOSE: u32 = 32768;
pub const MS_SILENT: u32 = 32768;
/// VFS does not apply the umask
pub const MS_POSIXACL: u32 = 1 << 16;
/// change to unbindable
pub const MS_UNBINDABLE: u32 = 1 << 17;
/// change to private
pub const MS_PRIVATE: u32 = 1 << 18;
/// change to slave
pub const MS_SLAVE: u32 = 1 << 19;
/// change to shared
pub const MS_SHARED: u32 = 1 << 20;
/// Update atime relative to mtime/ctime.
pub const MS_RELATIME: u32 = 1 << 21;
/// this is a `kern_mount` call
pub const MS_KERNMOUNT: u32 = 1 << 22;
/// Update inode `I_version` field
pub const MS_I_VERSION: u32 = 1 << 23;
/// Always perform atime updates
pub const MS_STRICTATIME: u32 = 1 << 24;
/// Update the on-disk `acm` times lazily
pub const MS_LAZYTIME: u32 = 1 << 25;

/// These sb flags are internal to the kernel
pub const MS_SUBMOUNT: u32 = 1 << 26;
pub const MS_NOREMOTELOCK: u32 = 1 << 27;
pub const MS_NOSEC: u32 = 1 << 28;
pub const MS_BORN: u32 = 1 << 29;
pub const MS_ACTIVE: u32 = 1 << 30;
pub const MS_NOUSER: u32 = 1 << 31;

///  Superblock flags that can be altered by `MS_REMOUNT`
pub const MS_RMT_MASK: u32 = MS_RDONLY | MS_SYNCHRONOUS | MS_MANDLOCK | MS_I_VERSION | MS_LAZYTIME;

/// Old magic mount flag and mask
pub const MS_MGC_VAL: u32 = 0xC0ED_0000;
pub const MS_MGC_MSK: u32 = 0xffff_0000;

/// `open_tree()` flags.
/// Clone the target tree and attach the clone
pub const OPEN_TREE_CLONE: u32 = 1;
/// Close the file on `execve()`
pub const OPEN_TREE_CLOEXEC: u32 = O_CLOEXEC as u32;

/// `move_mount()` flags.
/// Follow symlinks on from path
pub const MOVE_MOUNT_F_SYMLINKS: u32 = 0x0000_0001;
/// Follow automounts on from path
pub const MOVE_MOUNT_F_AUTOMOUNTS: u32 = 0x0000_0002;
/// Empty from path permitted
pub const MOVE_MOUNT_F_EMPTY_PATH: u32 = 0x0000_0004;
/// Follow symlinks on to path
pub const MOVE_MOUNT_T_SYMLINKS: u32 = 0x0000_0010;
/// Follow automounts on to path
pub const MOVE_MOUNT_T_AUTOMOUNTS: u32 = 0x0000_0020;
/// Empty to path permitted
pub const MOVE_MOUNT_T_EMPTY_PATH: u32 = 0x0000_0040;
/// Set sharing group instead
pub const MOVE_MOUNT_SET_GROUP: u32 = 0x0000_0100;
/// Mount beneath top mount
pub const MOVE_MOUNT_BENEATH: u32 = 0x0000_0200;
pub const MOVE_MOUNT__MASK: u32 = 0x0000_0377;

/// `fsopen()` flags.
pub const FSOPEN_CLOEXEC: u32 = 0x0000_0001;

/// `fspick()` flags.
pub const FSPICK_CLOEXEC: u32 = 0x0000_0001;
pub const FSPICK_SYMLINK_NOFOLLOW: u32 = 0x0000_0002;
pub const FSPICK_NO_AUTOMOUNT: u32 = 0x0000_0004;
pub const FSPICK_EMPTY_PATH: u32 = 0x0000_0008;

/// The type of `fsconfig()` call made.
pub type fsconfig_command_t = u32;
/// Set parameter, supplying no value
pub const FSCONFIG_SET_FLAG: fsconfig_command_t = 0;
/// Set parameter, supplying a string value
pub const FSCONFIG_SET_STRING: fsconfig_command_t = 1;
/// Set parameter, supplying a binary blob value
pub const FSCONFIG_SET_BINARY: fsconfig_command_t = 2;
/// Set parameter, supplying an object by path
pub const FSCONFIG_SET_PATH: fsconfig_command_t = 3;
/// Set parameter, supplying an object by (empty) path
pub const FSCONFIG_SET_PATH_EMPTY: fsconfig_command_t = 4;
/// Set parameter, supplying an object by fd
pub const FSCONFIG_SET_FD: fsconfig_command_t = 5;
/// Create new or reuse existing superblock
pub const FSCONFIG_CMD_CREATE: fsconfig_command_t = 6;
/// Invoke superblock reconfiguration
pub const FSCONFIG_CMD_RECONFIGURE: fsconfig_command_t = 7;
/// Create new superblock, fail if reusing existing superblock
pub const FSCONFIG_CMD_CREATE_EXCL: fsconfig_command_t = 8;

/// `fsmount()` flags.
pub const FSMOUNT_CLOEXEC: u32 = 0x0000_0001;

/// Mount attributes used in `fsmount()`.
/// Mount read-only
pub const MOUNT_ATTR_RDONLY: u32 = 0x0000_0001;
/// Ignore suid and sgid bits
pub const MOUNT_ATTR_NOSUID: u32 = 0x0000_0002;
/// Disallow access to device special files
pub const MOUNT_ATTR_NODEV: u32 = 0x0000_0004;
/// Disallow program execution
pub const MOUNT_ATTR_NOEXEC: u32 = 0x0000_0008;
/// Setting on how atime should be updated
pub const MOUNT_ATTR__ATIME: u32 = 0x0000_0070;
/// - Update atime relative to mtime/ctime.
pub const MOUNT_ATTR_RELATIME: u32 = 0x0000_0000;
/// - Do not update access times.
pub const MOUNT_ATTR_NOATIME: u32 = 0x0000_0010;
/// - Always perform atime updates
pub const MOUNT_ATTR_STRICTATIME: u32 = 0x0000_0020;
/// Do not update directory access times
pub const MOUNT_ATTR_NODIRATIME: u32 = 0x0000_0080;
/// Idmap mount to @`userns_fd` in struct `mount_attr`.
pub const MOUNT_ATTR_IDMAP: u32 = 0x0010_0000;
/// Do not follow symlinks
pub const MOUNT_ATTR_NOSYMFOLLOW: u32 = 0x0020_0000;

/// `mount_setattr()`
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct mount_attr_t {
    pub attr_set: u64,
    pub attr_clr: u64,
    pub program: u64,
    pub userns_fd: u64,
}

/// List of all `mount_attr` versions.
/// sizeof first published struct
pub const MOUNT_ATTR_SIZE_VER0: i32 = 32;

/// Structure for getting mount/superblock/filesystem info with `statmount(2)`.
///
/// The interface is similar to `statx(2)`: individual fields or groups can be
/// selected with the `mask` argument of `statmount()`.
/// Kernel will set the `mask` field according to the supported fields.
///
/// If string fields are selected, then the caller needs to pass a buffer that
/// has space after the fixed part of the structure.
/// Nul terminated strings are copied there and offsets relative to `str_` are stored
/// in the relevant fields.
///
/// If the buffer is too small, then `EOVERFLOW` is returned.
/// The actually used size is returned in `size`.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct statmount_t {
    /// Total size, including strings
    pub size: u32,
    __spare1: u32,

    /// What results were written
    pub mask: u64,
    /// Device ID
    pub sb_dev_major: u32,
    pub sb_dev_minor: u32,

    /// ...`_SUPER_MAGIC`
    pub sb_magic: u64,
    /// `SB_{RDONLY,SYNCHRONOUS,DIRSYNC,LAZYTIME}`
    pub sb_flags: u32,
    /// `[str]` Filesystem type
    pub fs_type: u32,
    /// Unique ID of mount
    pub mnt_id: u64,
    /// Unique ID of parent (for `root == mnt_id`)
    pub mnt_parent_id: u64,
    /// Reused IDs used in proc/.../mountinfo
    pub mnt_id_old: u32,
    pub mnt_parent_id_old: u32,
    /// `MOUNT_ATTR`_...
    pub mnt_attr: u64,
    /// `MS_{SHARED,SLAVE,PRIVATE,UNBINDABLE}`
    pub mnt_propagation: u64,
    /// ID of shared peer group
    pub mnt_peer_group: u64,
    /// Mount receives propagation from this ID
    pub mnt_master: u64,
    /// Propagation from in current namespace
    pub propagate_from: u64,
    /// [str] Root of mount relative to root of fs
    pub mnt_root: u32,
    /// [str] Mountpoint relative to current root
    pub mnt_point: u32,
    //__spare2: [u64; 50],
    __spare2: [u128; 25],
    /// Variable size part containing strings
    pub str_: [u8; 1],
}

/// Structure for passing mount ID and miscellaneous parameters to `statmount(2)`
/// and `listmount(2)`.
///
/// For `statmount(2)` `param` represents the request mask.
/// For `listmount(2)` `param` represents the last listed mount id (or zero).
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct mnt_id_req_t {
    pub size: u32,
    pub spare: u32,
    pub mnt_id: u64,
    pub param: u64,
}

/// List of all `mnt_id_req` versions.
/// sizeof first published struct
pub const MNT_ID_REQ_SIZE_VER0: i32 = 24;

/// `mask` bits for `statmount(2)`
/// Want/got sb_...
pub const STATMOUNT_SB_BASIC: u32 = 0x0000_0001;
/// Want/got mnt_...
pub const STATMOUNT_MNT_BASIC: u32 = 0x0000_0002;
/// Want/got `propagate_from`
pub const STATMOUNT_PROPAGATE_FROM: u32 = 0x0000_0004;
/// Want/got `mnt_root`
pub const STATMOUNT_MNT_ROOT: u32 = 0x0000_0008;
/// Want/got `mnt_point`
pub const STATMOUNT_MNT_POINT: u32 = 0x0000_0010;
/// Want/got `fs_type`
pub const STATMOUNT_FS_TYPE: u32 = 0x0000_0020;

/// Special `mnt_id` values that can be passed to listmount
/// root mount
pub const LSMT_ROOT: u64 = 0xffff_ffff_ffff_ffff;
