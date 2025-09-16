// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/fcntl.h`
//!
//! This file includes the definitions for open and fcntl
//! described by POSIX for <fcntl.h>; it also includes
//! related kernel definitions.

use crate::{off_t, pid_t};

/// File status flags: these are used by `open(2)`, `fcntl(2)`.
///
/// They are also used (indirectly) in the kernel file structure `f_flags`,
/// which is a superset of the open/fcntl flags.  Open flags and `f_flags`
/// are inter-convertible using `OFLAGS(fflags)` and `FFLAGS(oflags)`.
/// Open/fcntl flags begin with `O_`; kernel-internal flags begin with F.
/// open-only flags
/// open for reading only
pub const O_RDONLY: i32 = 0x0000;
/// open for writing only
pub const O_WRONLY: i32 = 0x0001;
/// open for reading and writing
pub const O_RDWR: i32 = 0x0002;
/// mask for above modes
pub const O_ACCMODE: i32 = 0x0003;

/// Kernel encoding of open mode; separate read and write bits that are
/// independently testable: 1 greater than the above.
pub const FREAD: i32 = 0x0001;
pub const FWRITE: i32 = 0x0002;

/// no delay
pub const O_NONBLOCK: i32 = 0x0004;
/// set append mode
pub const O_APPEND: i32 = 0x0008;

/// open with shared file lock
pub const O_SHLOCK: i32 = 0x0010;
/// open with exclusive file lock
pub const O_EXLOCK: i32 = 0x0020;
/// signal pgrp when data ready
pub const O_ASYNC: i32 = 0x0040;
/// synchronous writes
pub const O_FSYNC: i32 = 0x0080;

/// POSIX synonym for `O_FSYNC`
pub const O_SYNC: i32 = 0x0080;
/// don't follow symlinks
pub const O_NOFOLLOW: i32 = 0x0100;

/// create if nonexistent
pub const O_CREAT: i32 = 0x0200;
/// truncate to zero length
pub const O_TRUNC: i32 = 0x0400;
/// error if already exists
pub const O_EXCL: i32 = 0x0800;
/// descriptor holds advisory lock
pub const FHASLOCK: i32 = 0x4000;

/// Defined by POSIX 1003.1; BSD default, but must be distinct from `O_RDONLY`.
///
/// don't assign controlling terminal
pub const O_NOCTTY: i32 = 0x8000;

/// Attempt to bypass buffer cache
pub const O_DIRECT: i32 = 0x0001_0000;

/// Fail if not directory
pub const O_DIRECTORY: i32 = 0x0002_0000;
/// Open for execute only
pub const O_EXEC: i32 = 0x0004_0000;
pub const O_SEARCH: i32 = O_EXEC;

pub const FEXEC: i32 = O_EXEC;
pub const FSEARCH: i32 = O_SEARCH;

/// Defined by POSIX 1003.1-2008; BSD default, but reserve for future use.
///
/// Restore default termios attributes
pub const O_TTY_INIT: i32 = 0x0008_0000;

pub const O_CLOEXEC: i32 = 0x0010_0000;

/// open only after verification
pub const O_VERIFY: i32 = 0x0020_0000;
/// fd is only a path
pub const O_PATH: i32 = 0x0040_0000;
/// Do not allow name resolution to walk out of cwd
pub const O_RESOLVE_BENEATH: i32 = 0x0080_0000;

/// POSIX data sync
pub const O_DSYNC: i32 = 0x0100_0000;
pub const O_EMPTY_PATH: i32 = 0x0200_0000;

/// Only for devfs `d_close()` flags.
pub const FLASTCLOSE: i32 = O_DIRECTORY;
pub const FREVOKE: i32 = O_VERIFY;
/// Only for `fo_close()` from half-succeeded open
pub const FOPENFAILED: i32 = O_TTY_INIT;
/// Only for `O_PATH` files which passed ACCESS FREAD check on open
pub const FKQALLOWED: i32 = O_RESOLVE_BENEATH;

/// bits to save after open
pub const FMASK: i32 =
    FREAD | FWRITE | FAPPEND | FASYNC | FFSYNC | FDSYNC | FNONBLOCK | O_DIRECT | FEXEC | O_PATH;
/// bits settable by fcntl(`F_SETFL`, ...)
pub const FCNTLFLAGS: i32 = FAPPEND | FASYNC | FFSYNC | FDSYNC | FNONBLOCK | FRDAHEAD | O_DIRECT;

/// The O_* flags used to have only F* names, which were used in the kernel
/// and by fcntl.  We retain the F* names for the kernel `f_flag` field
/// and for backward compatibility for fcntl.  These flags are deprecated.
/// kernel/compat
pub const FAPPEND: i32 = O_APPEND;
/// kernel/compat
pub const FASYNC: i32 = O_ASYNC;
/// kernel
pub const FFSYNC: i32 = O_FSYNC;
/// kernel
pub const FDSYNC: i32 = O_DSYNC;
/// kernel
pub const FNONBLOCK: i32 = O_NONBLOCK;
/// compat
pub const FNDELAY: i32 = O_NONBLOCK;
/// compat
pub const O_NDELAY: i32 = O_NONBLOCK;

/// Historically, we ran out of bits in `f_flag` (which was once a short).
/// However, the flag bits not set in FMASK are only meaningful in the
/// initial open syscall.  Those bits were thus given a
/// different meaning for `fcntl(2)`.
/// Read ahead
pub const FRDAHEAD: i32 = O_CREAT;

/// Magic value that specify the use of the current working directory
/// to determine the target of relative file paths in the `openat()` and
/// similar syscalls.
pub const AT_FDCWD: i32 = -100;

// Miscellaneous flags for the *at() syscalls.
/// Check access using effective user and group ID
pub const AT_EACCESS: i32 = 0x0100;
/// Do not follow symbolic links
pub const AT_SYMLINK_NOFOLLOW: i32 = 0x0200;
/// Follow symbolic link
pub const AT_SYMLINK_FOLLOW: i32 = 0x0400;
/// Remove directory instead of file
pub const AT_REMOVEDIR: i32 = 0x0800;

// #define AT_UNUSED1		0x1000 *//* Was AT_BENEATH
/// Do not allow name resolution to walk out of dirfd
pub const AT_RESOLVE_BENEATH: i32 = 0x2000;
/// Operate on dirfd if path is empty
pub const AT_EMPTY_PATH: i32 = 0x4000;

// Constants used for fcntl(2)
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
pub const F_OGETLK: i32 = 7;
/// set record locking information
pub const F_OSETLK: i32 = 8;
/// `F_SETLK`; wait if blocked
pub const F_OSETLKW: i32 = 9;
/// duplicate file descriptor to arg
pub const F_DUP2FD: i32 = 10;

/// get record locking information
pub const F_GETLK: i32 = 11;
/// set record locking information
pub const F_SETLK: i32 = 12;
/// `F_SETLK`; wait if blocked
pub const F_SETLKW: i32 = 13;
/// debugging support for remote locks
pub const F_SETLK_REMOTE: i32 = 14;
/// read ahead
pub const F_READAHEAD: i32 = 15;
/// Darwin compatible read ahead
pub const F_RDAHEAD: i32 = 16;

/// Like `F_DUPFD`, but `FD_CLOEXEC` is set
pub const F_DUPFD_CLOEXEC: i32 = 17;

/// Like `F_DUP2FD`, but `FD_CLOEXEC` is set
pub const F_DUP2FD_CLOEXEC: i32 = 18;
pub const F_ADD_SEALS: i32 = 19;
pub const F_GET_SEALS: i32 = 20;
/// Kludge for libc, don't use it.
pub const F_ISUNIONSTACK: i32 = 21;

/// Seals (`F_ADD_SEALS`, `F_GET_SEALS`).
/// Prevent adding sealings
pub const F_SEAL_SEAL: i32 = 0x0001;
/// May not shrink
pub const F_SEAL_SHRINK: i32 = 0x0002;
/// May not grow
pub const F_SEAL_GROW: i32 = 0x0004;
/// May not write
pub const F_SEAL_WRITE: i32 = 0x0008;

/// file descriptor flags (`F_GETFD`, `F_SETFD`)
/// close-on-exec flag
pub const FD_CLOEXEC: i32 = 1;

/// record locking flags (`F_GETLK`, `F_SETLK`, `F_SETLKW`)
/// shared or read lock
pub const F_RDLCK: i32 = 1;
/// unlock
pub const F_UNLCK: i32 = 2;
/// exclusive or write lock
pub const F_WRLCK: i32 = 3;
/// purge locks for a given system ID
pub const F_UNLCKSYS: i32 = 4;
/// cancel an async lock request
pub const F_CANCEL: i32 = 5;

/// Wait until lock is granted
pub const F_WAIT: i32 = 0x010;
/// Use flock(2) semantics for lock
pub const F_FLOCK: i32 = 0x020;
/// Use POSIX semantics for lock
pub const F_POSIX: i32 = 0x040;
/// Lock owner is remote NFS client
pub const F_REMOTE: i32 = 0x080;
/// Ignore signals when waiting
pub const F_NOINTR: i32 = 0x100;
/// First right to advlock file
pub const F_FIRSTOPEN: i32 = 0x200;

/// Advisory file segment locking data type - information passed to system by user
#[repr(C)]
#[derive(Debug, Default)]
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

    /// remote system id or zero for local
    pub l_sysid: i32,
}

/// Old advisory file segment locking data type,
/// before adding `l_sysid`.
#[repr(C)]
#[derive(Debug, Default)]
pub struct __oflock_t {
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

/// Space control offset/length description
#[repr(C)]
#[derive(Debug, Default)]
pub struct spacectl_range_t {
    /// starting offset
    pub r_offset: off_t,

    /// length
    pub r_len: off_t,
}

// lock operations for flock(2)
/// shared file lock
pub const LOCK_SH: i32 = 0x01;
/// exclusive file lock
pub const LOCK_EX: i32 = 0x02;
/// don't block when locking
pub const LOCK_NB: i32 = 0x04;
/// unlock file
pub const LOCK_UN: i32 = 0x08;

// Advice to posix_fadvise
/// no special treatment
pub const POSIX_FADV_NORMAL: i32 = 0;
/// expect random page references
pub const POSIX_FADV_RANDOM: i32 = 1;
/// expect sequential page references
pub const POSIX_FADV_SEQUENTIAL: i32 = 2;
/// will need these pages
pub const POSIX_FADV_WILLNEED: i32 = 3;
/// dont need these pages
pub const POSIX_FADV_DONTNEED: i32 = 4;
/// access data only once
pub const POSIX_FADV_NOREUSE: i32 = 5;

/// Magic value that specify that corresponding file descriptor to filename
/// is unknown and sanitary check should be omitted in the funlinkat() and
/// similar syscalls.
pub const FD_NONE: i32 = -200;

// Commands for fspacectl(2)
/// deallocate space
pub const SPACECTL_DEALLOC: i32 = 1;

/// fspacectl(2) flags
pub const SPACECTL_F_SUPPORTED: i32 = 0;
