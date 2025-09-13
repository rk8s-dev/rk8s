// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/fcntl.h`
//!
//! This file includes the definitions for open and fcntl described by POSIX for <fcntl.h>

use crate::{O_DSYNC, O_SYNC};

/// File status flags: these are used by open(2), fcntl(2).
/// They are also used (indirectly) in the kernel file structure `f_flags`,
/// which is a superset of the open/fcntl flags.  Open flags and `f_flags`
/// are inter-convertible using OFLAGS(fflags) and FFLAGS(oflags).
/// Open/fcntl flags begin with O_; kernel-internal flags begin with F.
/// open-only flags
/// open for reading only
pub const O_RDONLY: i32 = 0x0000;
/// open for writing only
pub const O_WRONLY: i32 = 0x0001;
/// open for reading and writing
pub const O_RDWR: i32 = 0x0002;
/// mask for above modes
pub const O_ACCMODE: i32 = 0x0003;

pub const FREAD: i32 = 0x0000_0001;
pub const FWRITE: i32 = 0x0000_0002;
/// no delay
pub const O_NONBLOCK: i32 = 0x0000_0004;
/// set append mode
pub const O_APPEND: i32 = 0x0000_0008;

/// open with shared file lock
pub const O_SHLOCK: i32 = 0x0000_0010;
/// open with exclusive file lock
pub const O_EXLOCK: i32 = 0x0000_0020;
/// signal pgrp when data ready
pub const O_ASYNC: i32 = 0x0000_0040;
/// source compatibility: do not use
pub const O_FSYNC: i32 = O_SYNC;
/// don't follow symlinks
pub const O_NOFOLLOW: i32 = 0x0000_0100;
/// create if nonexistant
pub const O_CREAT: i32 = 0x0000_0200;
/// truncate to zero length
pub const O_TRUNC: i32 = 0x0000_0400;
/// error if already exists
pub const O_EXCL: i32 = 0x0000_0800;

/// descriptor requested for event notifications only
pub const O_EVTONLY: i32 = 0x0000_8000;

/// don't assign controlling terminal
pub const O_NOCTTY: i32 = 0x0002_0000;

pub const O_DIRECTORY: i32 = 0x0010_0000;
/// allow open of a symlink
pub const O_SYMLINK: i32 = 0x0020_0000;

// synch I/O data integrity
//pub const O_DSYNC: i32 = 0x0040_0000;

/// implicitly set `FD_CLOEXEC`
pub const O_CLOEXEC: i32 = 0x0100_0000;

/// no symlinks allowed in path
pub const O_NOFOLLOW_ANY: i32 = 0x2000_0000;

/// Descriptor value for the current working directory
pub const AT_FDCWD: i32 = -2;

/// Flags for the at functions
///
/// Use effective ids in access check
pub const AT_EACCESS: i32 = 0x0010;
/// Act on the symlink itself not the target
pub const AT_SYMLINK_NOFOLLOW: i32 = 0x0020;
/// Act on target of symlink
pub const AT_SYMLINK_FOLLOW: i32 = 0x0040;
/// Path refers to directory
pub const AT_REMOVEDIR: i32 = 0x0080;
/// Return real device inodes resides on for fstatat(2)
pub const AT_REALDEV: i32 = 0x0200;
/// Use only the fd and Ignore the path for fstatat(2)
pub const AT_FDONLY: i32 = 0x0400;

pub const O_DP_GETRAWENCRYPTED: i32 = 0x0001;
pub const O_DP_GETRAWUNENCRYPTED: i32 = 0x0002;

/// The O_* flags used to have only F* names, which were used in the kernel
/// and by fcntl.  We retain the F* names for the kernel `f_flags` field
/// and for backward compatibility for fcntl.
/// kernel/compat
pub const FAPPEND: i32 = O_APPEND;
/// kernel/compat
pub const FASYNC: i32 = O_ASYNC;
/// kernel
pub const FFSYNC: i32 = O_FSYNC;
/// kernel
pub const FFDSYNC: i32 = O_DSYNC;
/// kernel
pub const FNONBLOCK: i32 = O_NONBLOCK;
/// compat
pub const FNDELAY: i32 = O_NONBLOCK;
/// compat
pub const O_NDELAY: i32 = O_NONBLOCK;

/// Flags used for copyfile(2)
pub const CPF_OVERWRITE: i32 = 0x0001;
pub const CPF_IGNORE_MODE: i32 = 0x0002;

/// Constants used for fcntl(2)
///
/// command values
/// duplicate file descriptor
pub const F_DUPFD: i32 = 0;
/// get file descriptor flags
pub const F_GETFD: i32 = 1;
/// set file descriptor flags
pub const F_SETFD: i32 = 2;
/// get file status flags
pub const F_GETFL: i32 = 3;
/// set file status flags
pub const F_SETFL: i32 = 4;
/// get SIGIO/SIGURG proc/pgrp
pub const F_GETOWN: i32 = 5;
/// set SIGIO/SIGURG proc/pgrp
pub const F_SETOWN: i32 = 6;
/// get record locking information
pub const F_GETLK: i32 = 7;
/// set record locking information
pub const F_SETLK: i32 = 8;
/// `F_SETLK`; wait if blocked
pub const F_SETLKW: i32 = 9;
/// `F_SETLK`; wait if blocked, return on timeout
pub const F_SETLKWTIMEOUT: i32 = 10;
pub const F_FLUSH_DATA: i32 = 40;
/// Used for regression test
pub const F_CHKCLEAN: i32 = 41;
/// Preallocate storage
pub const F_PREALLOCATE: i32 = 42;
/// Truncate a file. Equivalent to calling truncate(2)
pub const F_SETSIZE: i32 = 43;
/// Issue an advisory read async with no copy to user
pub const F_RDADVISE: i32 = 44;
/// turn read ahead off/on for this fd
pub const F_RDAHEAD: i32 = 45;
// 46,47 used to be F_READBOOTSTRAP and F_WRITEBOOTSTRAP
/// turn data caching off/on for this fd
pub const F_NOCACHE: i32 = 48;
/// file offset to device offset
pub const F_LOG2PHYS: i32 = 49;
/// return the full path of the fd
pub const F_GETPATH: i32 = 50;
/// fsync + ask the drive to flush to the media
pub const F_FULLFSYNC: i32 = 51;
/// find which component (if any) is a package
pub const F_PATHPKG_CHECK: i32 = 52;
/// "freeze" all fs operations
pub const F_FREEZE_FS: i32 = 53;
/// "thaw" all fs operations
pub const F_THAW_FS: i32 = 54;
/// turn data caching off/on (globally) for this file
pub const F_GLOBAL_NOCACHE: i32 = 55;

/// add detached signatures
pub const F_ADDSIGS: i32 = 59;

/// add signature from same file (used by dyld for shared libs)
pub const F_ADDFILESIGS: i32 = 61;

/// used in conjunction with `F_NOCACHE` to indicate that DIRECT, synchonous writes
///
/// should not be used (i.e. its ok to temporaily create cached pages)
pub const F_NODIRECT: i32 = 62;

/// Get the protection class of a file from the EA, returns int
pub const F_GETPROTECTIONCLASS: i32 = 63;
/// Set the protection class of a file for the EA, requires int
pub const F_SETPROTECTIONCLASS: i32 = 64;

/// file offset to device offset, extended
pub const F_LOG2PHYS_EXT: i32 = 65;

/// get record locking information, per-process
pub const F_GETLKPID: i32 = 66;

// See F_DUPFD_CLOEXEC below for 67

/// Mark the file as being the backing store for another filesystem
pub const F_SETBACKINGSTORE: i32 = 70;
/// return the full path of the FD, but error in specific mtmd circumstances
pub const F_GETPATH_MTMINFO: i32 = 71;

/// Returns the code directory, with associated hashes, to the caller
pub const F_GETCODEDIR: i32 = 72;

/// No SIGPIPE generated on EPIPE
pub const F_SETNOSIGPIPE: i32 = 73;
/// Status of SIGPIPE for this fd
pub const F_GETNOSIGPIPE: i32 = 74;

/// For some cases, we need to rewrap the key for AKS/MKB
pub const F_TRANSCODEKEY: i32 = 75;

/// file being written to a by single writer... if throttling enabled, writes
///
/// may be broken into smaller chunks with throttling in between
pub const F_SINGLE_WRITER: i32 = 76;

/// Get the protection version number for this filesystem
pub const F_GETPROTECTIONLEVEL: i32 = 77;

/// Add detached code signatures (used by dyld for shared libs)
pub const F_FINDSIGS: i32 = 78;

/// Add signature from same file, only if it is signed by Apple (used by dyld for simulator)
pub const F_ADDFILESIGS_FOR_DYLD_SIM: i32 = 83;

/// fsync + issue barrier to drive
pub const F_BARRIERFSYNC: i32 = 85;

/// Add signature from same file, return end offset in structure on success
pub const F_ADDFILESIGS_RETURN: i32 = 97;
/// Check if Library Validation allows this Mach-O file to be mapped into the calling process
pub const F_CHECK_LV: i32 = 98;

/// Deallocate a range of the file
pub const F_PUNCHHOLE: i32 = 99;

/// Trim an active file
pub const F_TRIM_ACTIVE_FILE: i32 = 100;

/// Synchronous advisory read fcntl for regular and compressed file
pub const F_SPECULATIVE_READ: i32 = 101;

/// return the full path without firmlinks of the fd
pub const F_GETPATH_NOFIRMLINK: i32 = 102;

/// Add signature from same file, return information
pub const F_ADDFILESIGS_INFO: i32 = 103;
/// Add supplemental signature from same file with fd reference to original
pub const F_ADDFILESUPPL: i32 = 104;
/// Look up code signature information attached to a file or slice
pub const F_GETSIGSINFO: i32 = 105;

/// FS-specific fcntl()'s numbers begin at 0x00010000 and go up
pub const FCNTL_FS_SPECIFIC_BASE: i32 = 0x0001_0000;

/// mark the dup with `FD_CLOEXEC`
pub const F_DUPFD_CLOEXEC: i32 = 67;

/// file descriptor flag
/// close-on-exec flag
pub const FD_CLOEXEC: i32 = 1;

/// record locking flags
///
/// shared or read lock
pub const F_RDLCK: i32 = 1;
/// unlock
pub const F_UNLCK: i32 = 2;
/// exclusive or write lock
pub const F_WRLCK: i32 = 3;

/// allocate flags (`F_PREALLOCATE`)
///
/// allocate contigious space
pub const F_ALLOCATECONTIG: i32 = 0x0000_0002;
/// allocate all requested space or no space at all
pub const F_ALLOCATEALL: i32 = 0x0000_0004;

/// Position Modes (`fst_posmode`) for `F_PREALLOCATE`
///
/// Make it past all of the SEEK pos modes so that
///
/// we can keep them in sync should we desire
pub const F_PEOFPOSMODE: i32 = 3;
/// specify volume starting postion
pub const F_VOLPOSMODE: i32 = 4;

//*
// * Advisory file segment locking data type -
// * information passed to system by user
// */
//struct flock {
//	off_t   l_start;        /* starting offset */
//	off_t   l_len;          /* len = 0 means until end of file */
//	pid_t   l_pid;          /* lock owner */
//	short   l_type;         /* lock type: read/write, etc. */
//	short   l_whence;       /* type of l_start */
//};
//
//#include <sys/_types/_timespec.h>
//
//#if __DARWIN_C_LEVEL >= __DARWIN_C_FULL
//*
// * Advisory file segment locking with time out -
// * Information passed to system by user for F_SETLKWTIMEOUT
// */
//struct flocktimeout {
//	struct flock    fl;             /* flock passed for file locking */
//	struct timespec timeout;        /* timespec struct for timeout */
//};
//#endif /* __DARWIN_C_LEVEL >= __DARWIN_C_FULL */
//
//#if !defined(_POSIX_C_SOURCE) || defined(_DARWIN_C_SOURCE)
//*
// * advisory file read data type -
// * information passed by user to system
// */
//
//
//struct radvisory {
//	off_t   ra_offset;
//	int     ra_count;
//};
//
//
//*
// * detached code signatures data type -
// * information passed by user to system used by F_ADDSIGS and F_ADDFILESIGS.
// * F_ADDFILESIGS is a shortcut for files that contain their own signature and
// * doesn't require mapping of the file in order to load the signature.
// */
//pub const USER_FSIGNATURES_CDHASH_LEN: i32 = 20;
//typedef struct fsignatures {
//	off_t           fs_file_start;
//	void            *fs_blob_start;
//	size_t          fs_blob_size;
//
//	/* The following fields are only applicable to F_ADDFILESIGS_INFO (64bit only). */
//	/* Prior to F_ADDFILESIGS_INFO, this struct ended after fs_blob_size. */
//	size_t          fs_fsignatures_size;// input: size of this struct (for compatibility)
//	char            fs_cdhash[USER_FSIGNATURES_CDHASH_LEN];     // output: cdhash
//	int             fs_hash_type;// output: hash algorithm type for cdhash
//} fsignatures_t;
//
//typedef struct fsupplement {
//	off_t           fs_file_start;   /* offset of Mach-O image in FAT file  */
//	off_t           fs_blob_start;   /* offset of signature in Mach-O image */
//	size_t          fs_blob_size;    /* signature blob size                 */
//	int             fs_orig_fd;      /* address of original image           */
//} fsupplement_t;
//
//
//
//*
// * DYLD needs to check if the object is allowed to be combined
// * into the main binary. This is done between the code signature
// * is loaded and dyld is doing all the work to process the LOAD commands.
// *
// * While this could be done in F_ADDFILESIGS.* family the hook into
// * the MAC module doesn't say no when LV isn't enabled and then that
// * is cached on the vnode, and the MAC module never gets change once
// * a process that library validation enabled.
// */
//typedef struct fchecklv {
//	off_t           lv_file_start;
//	size_t          lv_error_message_size;
//	void            *lv_error_message;
//} fchecklv_t;
//
//
//* At this time F_GETSIGSINFO can only indicate platformness.
// *  As additional requestable information is defined, new keys will be added and the
// *  fgetsigsinfo_t structure will be lengthened to add space for the additional information
// */
//pub const GETSIGSINFO_PLATFORM_BINARY: i32 = 1;
//
///// fgetsigsinfo_t used by F_GETSIGSINFO command
//typedef struct fgetsigsinfo {
//	off_t fg_file_start; /* IN: Offset in the file to look for a signature, -1 for any signature */
//	int   fg_info_request; /* IN: Key indicating the info requested */
//	int   fg_sig_is_platform; /* OUT: 1 if the signature is a plat form binary, 0 if not */
//} fgetsigsinfo_t;
//

/// lock operations for flock(2)
/// shared file lock
pub const LOCK_SH: i32 = 0x01;
/// exclusive file lock
pub const LOCK_EX: i32 = 0x02;
/// don't block when locking
pub const LOCK_NB: i32 = 0x04;
/// unlock file
pub const LOCK_UN: i32 = 0x08;

/// force window to popup on open
#[allow(overflowing_literals)]
pub const O_POPUP: i32 = 0x8000_0000;
/// small, clean popup window
pub const O_ALERT: i32 = 0x2000_0000;

pub type filesec_property_t = i32;
pub const FILESEC_OWNER: filesec_property_t = 1;
pub const FILESEC_GROUP: filesec_property_t = 2;
pub const FILESEC_UUID: filesec_property_t = 3;
pub const FILESEC_MODE: filesec_property_t = 4;
pub const FILESEC_ACL: filesec_property_t = 5;
pub const FILESEC_GRPUUID: filesec_property_t = 6;

/// these are private to the implementation
pub const FILESEC_ACL_RAW: filesec_property_t = 100;
pub const FILESEC_ACL_ALLOCSIZE: filesec_property_t = 101;

/// backwards compatibility
pub const FILESEC_GUID: filesec_property_t = FILESEC_UUID;
