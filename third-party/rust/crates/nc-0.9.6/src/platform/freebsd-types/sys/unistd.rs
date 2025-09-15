// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/unistd.h`

#![allow(clippy::unreadable_literal)]

/// POSIX options and option groups we unconditionally do or don't
/// implement.  Those options which are implemented (or not) entirely
/// in user mode are defined in <unistd.h>.  Please keep this list in
/// alphabetical order.
///
/// Anything which is defined as zero below **must** have an
/// implementation for the corresponding sysconf() which is able to
/// determine conclusively whether or not the feature is supported.
/// Anything which is defined as other than -1 below **must** have
/// complete headers, types, and function declarations as specified by
/// the POSIX standard; however, if the relevant sysconf() function
/// returns -1, the functions may be stubbed out.
pub const _POSIX_ADVISORY_INFO: i32 = 200112;
pub const _POSIX_ASYNCHRONOUS_IO: i32 = 200112;
pub const _POSIX_CHOWN_RESTRICTED: i32 = 1;
pub const _POSIX_CLOCK_SELECTION: i32 = -1;
pub const _POSIX_CPUTIME: i32 = 200112;
pub const _POSIX_FSYNC: i32 = 200112;
pub const _POSIX_IPV6: i32 = 0;
pub const _POSIX_JOB_CONTROL: i32 = 1;
pub const _POSIX_MAPPED_FILES: i32 = 200112;
pub const _POSIX_MEMLOCK: i32 = -1;
pub const _POSIX_MEMLOCK_RANGE: i32 = 200112;
pub const _POSIX_MEMORY_PROTECTION: i32 = 200112;
pub const _POSIX_MESSAGE_PASSING: i32 = 200112;
pub const _POSIX_MONOTONIC_CLOCK: i32 = 200112;
pub const _POSIX_NO_TRUNC: i32 = 1;
pub const _POSIX_PRIORITIZED_IO: i32 = -1;
pub const _POSIX_PRIORITY_SCHEDULING: i32 = 0;
pub const _POSIX_RAW_SOCKETS: i32 = 200112;
pub const _POSIX_REALTIME_SIGNALS: i32 = 200112;
pub const _POSIX_SEMAPHORES: i32 = 200112;
pub const _POSIX_SHARED_MEMORY_OBJECTS: i32 = 200112;
pub const _POSIX_SPORADIC_SERVER: i32 = -1;
pub const _POSIX_SYNCHRONIZED_IO: i32 = -1;
pub const _POSIX_TIMEOUTS: i32 = 200112;
pub const _POSIX_TIMERS: i32 = 200112;
pub const _POSIX_TYPED_MEMORY_OBJECTS: i32 = -1;
pub const _POSIX_VDISABLE: i32 = 0xff;

pub const _XOPEN_SHM: i32 = 1;
pub const _XOPEN_STREAMS: i32 = -1;

/// Although we have saved user/group IDs, we do not use them in setuid
/// as described in POSIX 1003.1, because the feature does not work for
/// root.  We use the saved IDs in seteuid/setegid, which are not currently
/// part of the POSIX 1003.1 specification.
///
/// saved set-user-ID and set-group-ID
pub const _POSIX_SAVED_IDS: i32 = 1;

/// Define the POSIX.1 version we target for compliance.
pub const _POSIX_VERSION: i32 = 200112;

/// access function
/// test for existence of file
pub const F_OK: i32 = 0;
/// test for execute or search permission
pub const X_OK: i32 = 0x01;
/// test for write permission
pub const W_OK: i32 = 0x02;
/// test for read permission
pub const R_OK: i32 = 0x04;

/// whence values for lseek(2)
/// set file offset to offset
pub const SEEK_SET: i32 = 0;
/// set file offset to current plus offset
pub const SEEK_CUR: i32 = 1;
/// set file offset to EOF plus offset
pub const SEEK_END: i32 = 2;

/// set file offset to next data past offset
pub const SEEK_DATA: i32 = 3;
/// set file offset to next hole past offset
pub const SEEK_HOLE: i32 = 4;

/// whence values for lseek(2); renamed by POSIX 1003.1
pub const L_SET: i32 = SEEK_SET;
pub const L_INCR: i32 = SEEK_CUR;
pub const L_XTND: i32 = SEEK_END;

/// configurable pathname variables
pub const _PC_LINK_MAX: i32 = 1;
pub const _PC_MAX_CANON: i32 = 2;
pub const _PC_MAX_INPUT: i32 = 3;
pub const _PC_NAME_MAX: i32 = 4;
pub const _PC_PATH_MAX: i32 = 5;
pub const _PC_PIPE_BUF: i32 = 6;
pub const _PC_CHOWN_RESTRICTED: i32 = 7;
pub const _PC_NO_TRUNC: i32 = 8;
pub const _PC_VDISABLE: i32 = 9;

pub const _PC_ASYNC_IO: i32 = 53;
pub const _PC_PRIO_IO: i32 = 54;
pub const _PC_SYNC_IO: i32 = 55;

pub const _PC_ALLOC_SIZE_MIN: i32 = 10;
pub const _PC_FILESIZEBITS: i32 = 12;
pub const _PC_REC_INCR_XFER_SIZE: i32 = 14;
pub const _PC_REC_MAX_XFER_SIZE: i32 = 15;
pub const _PC_REC_MIN_XFER_SIZE: i32 = 16;
pub const _PC_REC_XFER_ALIGN: i32 = 17;
pub const _PC_SYMLINK_MAX: i32 = 18;

pub const _PC_ACL_EXTENDED: i32 = 59;
pub const _PC_ACL_PATH_MAX: i32 = 60;
pub const _PC_CAP_PRESENT: i32 = 61;
pub const _PC_INF_PRESENT: i32 = 62;
pub const _PC_MAC_PRESENT: i32 = 63;
pub const _PC_ACL_NFS4: i32 = 64;
pub const _PC_DEALLOC_PRESENT: i32 = 65;

/// From `OpenSolaris`, used by `SEEK_DATA/SEEK_HOLE`.
pub const _PC_MIN_HOLE_SIZE: i32 = 21;

/// rfork() options.
///
/// Currently, some operations without RFPROC set are not supported.
/// UNIMPL new plan9 `name space`
pub const RFNAMEG: i32 = 1 << 0;
/// UNIMPL copy plan9 `env space`
pub const RFENVG: i32 = 1 << 1;
/// copy fd table
pub const RFFDG: i32 = 1 << 2;
/// UNIMPL create new plan9 `note group`
pub const RFNOTEG: i32 = 1 << 3;
/// change child (else changes curproc)
pub const RFPROC: i32 = 1 << 4;
/// share `address space`
pub const RFMEM: i32 = 1 << 5;
/// give child to init
pub const RFNOWAIT: i32 = 1 << 6;
/// UNIMPL zero plan9 `name space`
pub const RFCNAMEG: i32 = 1 << 10;
/// UNIMPL zero plan9 `env space`
pub const RFCENVG: i32 = 1 << 11;
/// close all fds, zero fd table
pub const RFCFDG: i32 = 1 << 12;
/// enable kernel thread support
pub const RFTHREAD: i32 = 1 << 13;
/// share signal handlers
pub const RFSIGSHARE: i32 = 1 << 14;
/// do linux clone exit parent notification
pub const RFLINUXTHPN: i32 = 1 << 16;
/// leave child in a stopped state
pub const RFSTOPPED: i32 = 1 << 17;
/// use a pid higher than 10 (idleproc)
pub const RFHIGHPID: i32 = 1 << 18;
/// select signal for exit parent notification
pub const RFTSIGZMB: i32 = 1 << 19;

/// selected signal number is in bits 20-27
pub const RFTSIGSHIFT: i32 = 20;
pub const RFTSIGMASK: i32 = 0xff;
/// return a process descriptor
pub const RFPROCDESC: i32 = 1 << 28;
/// kernel: parent sleeps until child exits (vfork)
pub const RFPPWAIT: i32 = 1 << 31;
/// user: vfork(2) semantics, clear signals
pub const RFSPAWN: i32 = 1 << 31;
pub const RFFLAGS: i32 = RFFDG
    | RFPROC
    | RFMEM
    | RFNOWAIT
    | RFCFDG
    | RFTHREAD
    | RFSIGSHARE
    | RFLINUXTHPN
    | RFSTOPPED
    | RFHIGHPID
    | RFTSIGZMB
    | RFPROCDESC
    | RFSPAWN
    | RFPPWAIT;
pub const RFKERNELONLY: i32 = RFSTOPPED | RFHIGHPID | RFPROCDESC;
