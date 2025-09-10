// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/fcntl.h`
//!
//! This file includes the definitions for open and fcntl
//! described by POSIX for <fcntl.h>; it also includes
//! related kernel definitions.

#![allow(overflowing_literals)]

use super::{off_t, pid_t};

/// File status flags: these are used by open(2), fcntl(2).
/// They are also used (indirectly) in the kernel file structure `f_flags`,
/// which is a superset of the open/fcntl flags.  Open flags and `f_flags`
/// are inter-convertible using OFLAGS(fflags) and FFLAGS(oflags).
/// Open/fcntl flags begin with `O_`; kernel-internal flags begin with F.
///
/// open-only flags
///
/// open for reading only
pub const O_RDONLY: i32 = 0x0000_0000;
/// open for writing only
pub const O_WRONLY: i32 = 0x0000_0001;
/// open for reading and writing
pub const O_RDWR: i32 = 0x0000_0002;
/// mask for above modes
pub const O_ACCMODE: i32 = 0x0000_0003;

/// Kernel encoding of open mode; separate read and write bits that are
/// independently testable: 1 greater than the above.
///
/// XXX
/// FREAD and FWRITE are excluded from the #ifdef _KERNEL so that TIOCFLUSH,
/// which was documented to use FREAD/FWRITE, continues to work.
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
/// synchronous writes
pub const O_SYNC: i32 = 0x0000_0080;
/// don't follow symlinks on the last
pub const O_NOFOLLOW: i32 = 0x0000_0100;
/// create if nonexistent
pub const O_CREAT: i32 = 0x0000_0200;
/// truncate to zero length
pub const O_TRUNC: i32 = 0x0000_0400;
/// error if already exists
pub const O_EXCL: i32 = 0x0000_0800;

/// defined by POSIX 1003.1; BSD default, but required to be bitwise distinct
/// don't assign controlling terminal
pub const O_NOCTTY: i32 = 0x0000_8000;

/// write: I/O data completion
pub const O_DSYNC: i32 = 0x0001_0000;
/// read: I/O completion as for write
pub const O_RSYNC: i32 = 0x0002_0000;

/// use alternate i/o semantics
pub const O_ALT_IO: i32 = 0x0004_0000;
/// direct I/O hint
pub const O_DIRECT: i32 = 0x0008_0000;

/// fail if not a directory
pub const O_DIRECTORY: i32 = 0x0020_0000;
/// set close on exec
pub const O_CLOEXEC: i32 = 0x0040_0000;
/// skip search permission checks
pub const O_SEARCH: i32 = 0x0080_0000;
/// don't deliver sigpipe
pub const O_NOSIGPIPE: i32 = 0x0100_0000;
/// fail if not a regular file
pub const O_REGULAR: i32 = 0x0200_0000;

/// all bits settable during open(2)
pub const O_MASK: i32 = O_ACCMODE
    | O_NONBLOCK
    | O_APPEND
    | O_SHLOCK
    | O_EXLOCK
    | O_ASYNC
    | O_SYNC
    | O_CREAT
    | O_TRUNC
    | O_EXCL
    | O_DSYNC
    | O_RSYNC
    | O_NOCTTY
    | O_ALT_IO
    | O_NOFOLLOW
    | O_DIRECT
    | O_DIRECTORY
    | O_CLOEXEC
    | O_NOSIGPIPE
    | O_REGULAR;

/// mark during gc()
pub const FMARK: i32 = 0x0000_1000;
/// defer for next gc pass
pub const FDEFER: i32 = 0x0000_2000;
/// descriptor holds advisory lock
pub const FHASLOCK: i32 = 0x0000_4000;
/// scan during gc passes
pub const FSCAN: i32 = 0x0010_0000;
/// suppress kernel error messages
pub const FSILENT: i32 = 0x4000_0000;
/// kernel originated ioctl
pub const FKIOCTL: i32 = 0x8000_0000;
/// bits settable by `fcntl(F_SETFL, ...)`
pub const FCNTLFLAGS: i32 =
    FAPPEND | FASYNC | FFSYNC | FNONBLOCK | FDSYNC | FRSYNC | FALTIO | FDIRECT | FNOSIGPIPE;
/// bits to save after `open(2)`
pub const FMASK: i32 = FREAD | FWRITE | FCNTLFLAGS;

/// The `O_*` flags used to have only F* names, which were used in the kernel
/// and by fcntl.  We retain the F* names for the kernel `f_flags` field
/// and for backward compatibility for fcntl.
/// kernel/compat
pub const FAPPEND: i32 = O_APPEND;
/// kernel/compat
pub const FASYNC: i32 = O_ASYNC;
/// compat
pub const O_FSYNC: i32 = O_SYNC;
/// compat
pub const FNDELAY: i32 = O_NONBLOCK;
/// compat
pub const O_NDELAY: i32 = O_NONBLOCK;

/// kernel
pub const FNOSIGPIPE: i32 = O_NOSIGPIPE;
/// kernel
pub const FNONBLOCK: i32 = O_NONBLOCK;
/// kernel
pub const FFSYNC: i32 = O_SYNC;
/// kernel
pub const FDSYNC: i32 = O_DSYNC;
/// kernel
pub const FRSYNC: i32 = O_RSYNC;
/// kernel
pub const FALTIO: i32 = O_ALT_IO;
/// kernel
pub const FDIRECT: i32 = O_DIRECT;

/// Constants used for `fcntl(2)`
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
/// close all fds >= to the one given
pub const F_CLOSEM: i32 = 10;
/// return the max open fd
pub const F_MAXFD: i32 = 11;
/// close on exec duplicated fd
pub const F_DUPFD_CLOEXEC: i32 = 12;
/// get SIGPIPE disposition
pub const F_GETNOSIGPIPE: i32 = 13;
/// set SIGPIPE disposition
pub const F_SETNOSIGPIPE: i32 = 14;

/// file descriptor flags `(F_GETFD, F_SETFD)`
/// close-on-exec flag
pub const FD_CLOEXEC: i32 = 1;

/// record locking flags `(F_GETLK, F_SETLK, F_SETLKW)`
/// shared or read lock
pub const F_RDLCK: i32 = 1;
/// unlock
pub const F_UNLCK: i32 = 2;
/// exclusive or write lock
pub const F_WRLCK: i32 = 3;
/// Wait until lock is granted
pub const F_WAIT: i32 = 0x010;
/// Use flock(2) semantics for lock
pub const F_FLOCK: i32 = 0x020;
/// Use POSIX semantics for lock
pub const F_POSIX: i32 = 0x040;

/// Constants for fcntl's passed to the underlying fs - like ioctl's.
pub const F_PARAM_MASK: i32 = 0xfff;
pub const F_PARAM_MAX: i32 = 4095;
/// This fcntl goes to the fs
pub const F_FSCTL: i32 = 0x8000_0000;
/// no parameters
pub const F_FSVOID: i32 = 0x4000_0000;
/// copy out parameter
pub const F_FSOUT: i32 = 0x2000_0000;
/// copy in parameter
pub const F_FSIN: i32 = 0x1000_0000;
pub const F_FSINOUT: i32 = F_FSIN | F_FSOUT;
/// mask for IN/OUT/VOID
pub const F_FSDIRMASK: i32 = 0x7000_0000;
/// command is fs-specific
pub const F_FSPRIV: i32 = 0x0000_8000;

/// Advisory file segment locking data type -
/// information passed to system by user
#[repr(C)]
pub struct flock_t {
    /// starting offset
    pub l_start: off_t,
    /// len = 0 means until end of file
    pub l_len: off_t,
    /// lock owner
    pub l_pid: pid_t,
    /// lock type: read/write, etc.
    pub l_type: i16,
    /// type of l_start
    pub l_whence: i16,
}

/// lock operations for flock(2)
/// shared file lock
pub const LOCK_SH: i32 = 0x01;
/// exclusive file lock
pub const LOCK_EX: i32 = 0x02;
/// don't block when locking
pub const LOCK_NB: i32 = 0x04;
/// unlock file
pub const LOCK_UN: i32 = 0x08;

/// set file offset to offset
pub const SEEK_SET: i32 = 0;
/// set file offset to current plus offset
pub const SEEK_CUR: i32 = 1;
/// set file offset to EOF plus offset
pub const SEEK_END: i32 = 2;

/// `posix_advise` advisories.
///
/// default advice / no advice
pub const POSIX_FADV_NORMAL: i32 = 0;
/// random access
pub const POSIX_FADV_RANDOM: i32 = 1;
/// sequential access(lower to higher)
pub const POSIX_FADV_SEQUENTIAL: i32 = 2;
/// be needed in near future
pub const POSIX_FADV_WILLNEED: i32 = 3;
/// not be needed in near future
pub const POSIX_FADV_DONTNEED: i32 = 4;
/// be accessed once
pub const POSIX_FADV_NOREUSE: i32 = 5;

/// Constants for X/Open Extended API set 2 (a.k.a. C063)
/// Use cwd for relative link target
pub const AT_FDCWD: i32 = -100;
/// Use euig/egid for access checks
pub const AT_EACCESS: i32 = 0x100;
/// Do not follow symlinks
pub const AT_SYMLINK_NOFOLLOW: i32 = 0x200;
/// Follow symlinks
pub const AT_SYMLINK_FOLLOW: i32 = 0x400;
/// Remove directory only
pub const AT_REMOVEDIR: i32 = 0x800;
