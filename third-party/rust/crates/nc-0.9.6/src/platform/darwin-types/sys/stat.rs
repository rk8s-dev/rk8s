// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/stat.h`

#![allow(overflowing_literals)]

use crate::{
    blkcnt_t, blksize_t, dev_t, gid_t, ino64_t, mode_t, nlink_t, off_t, timespec_t, uid_t, S_IRGRP,
    S_IROTH, S_IRUSR, S_IRWXG, S_IRWXO, S_IRWXU, S_ISGID, S_ISTXT, S_ISUID, S_IWGRP, S_IWOTH,
    S_IWUSR,
};

/// This structure is used as the second parameter to the fstat64(),
/// lstat64(), and stat64() functions, and for struct stat when
/// __DARWIN_64_BIT_INO_T is set. __DARWIN_STRUCT_STAT64 is defined
/// above, depending on whether we use struct timespec or the direct
/// components.
///
/// This is simillar to stat except for 64bit inode number
/// number instead of 32bit ino_t and the addition of create(birth) time.
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct stat64_t {
    /// ID of device containing file
    pub st_dev: dev_t,
    /// Mode of file (see below)
    pub st_mode: mode_t,
    /// Number of hard links
    pub st_nlink: nlink_t,
    /// File serial number
    pub st_ino: ino64_t,
    /// User ID of the file
    pub st_uid: uid_t,
    /// Group ID of the file
    pub st_gid: gid_t,
    /// Device ID
    pub st_rdev: dev_t,

    /// time of last access
    pub st_atimespec: timespec_t,
    /// time of last data modification
    pub st_mtimespec: timespec_t,
    /// time of last status change
    pub st_ctimespec: timespec_t,
    /// time of file creation(birth)
    pub st_birthtimespec: timespec_t,

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
    /// RESERVED: DO NOT USE!
    st_lspare: i32,
    /// RESERVED: DO NOT USE!
    st_qspare: [i64; 2],
}

/// This structure is used as the second parameter to the fstat(),
/// lstat(), and stat() functions.
pub type stat_t = stat64_t;

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

// The following bit is reserved for FreeBSD.  It is not implemented in Mac OS X.
//
// #define UF_NOUNLINK	0x00000010 */	/* file may not be removed or renamed
/// file is compressed (some file-systems)
pub const UF_COMPRESSED: i32 = 0x0000_0020;

/// UF_TRACKED is used for dealing with document IDs.  We no longer issue
/// notifications for deletes or renames for files which have UF_TRACKED set. */
pub const UF_TRACKED: i32 = 0x0000_0040;

/// entitlement required for reading and writing
pub const UF_DATAVAULT: i32 = 0x0000_0080;

/// Bits 0x0100 through 0x4000 are currently undefined.
/// hint that this item should not be displayed in a GUI
pub const UF_HIDDEN: i32 = 0x0000_8000;

/// Super-user changeable flags.
///
/// mask of superuser supported flags
pub const SF_SUPPORTED: i32 = 0x009f_0000;
/// mask of superuser changeable flags
pub const SF_SETTABLE: i32 = 0x3fff_0000;
/// mask of system read-only synthetic flags
pub const SF_SYNTHETIC: i32 = 0xc000_0000;
/// file is archived
pub const SF_ARCHIVED: i32 = 0x0001_0000;
/// file may not be changed
pub const SF_IMMUTABLE: i32 = 0x0002_0000;
/// writes to file may only append
pub const SF_APPEND: i32 = 0x0004_0000;
/// entitlement required for writing
pub const SF_RESTRICTED: i32 = 0x0008_0000;
/// Item may not be removed, renamed or mounted on
pub const SF_NOUNLINK: i32 = 0x0010_0000;

// The following two bits are reserved for FreeBSD.  They are not implemented in Mac OS X.
// #define SF_SNAPSHOT	0x00200000 */	/* snapshot inode
// NOTE: There is no SF_HIDDEN bit.
/// file is a firmlink
pub const SF_FIRMLINK: i32 = 0x0080_0000;

/// Synthetic flags.
///
/// These are read-only.  We keep them out of SF_SUPPORTED so that attempts to set them will fail.
///
/// file is dataless object
pub const SF_DATALESS: i32 = 0x4000_0000;

/// Extended flags ("EF") returned by ATTR_CMNEXT_EXT_FLAGS from getattrlist/getattrlistbulk
///
/// file may share blocks with another file
pub const EF_MAY_SHARE_BLOCKS: i32 = 0x0000_0001;
/// file has no xattrs at all
pub const EF_NO_XATTRS: i32 = 0x0000_0002;
/// file is a sync root for iCloud
pub const EF_IS_SYNC_ROOT: i32 = 0x0000_0004;
/// file is purgeable
pub const EF_IS_PURGEABLE: i32 = 0x0000_0008;
/// file has at least one sparse region
pub const EF_IS_SPARSE: i32 = 0x0000_0010;
/// a synthetic directory/symlink
pub const EF_IS_SYNTHETIC: i32 = 0x0000_0020;
