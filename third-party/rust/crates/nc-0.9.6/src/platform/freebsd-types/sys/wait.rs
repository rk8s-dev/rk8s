// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/wait.h`
//!
//! This file holds definitions relevant to the wait4 system call and the
//! alternate interfaces that use it (wait, wait3, waitpid).

/// Macros to test the exit status returned by wait and extract the relevant values.
pub const WCOREFLAG: i32 = 0200;

pub const fn _WSTATUS(x: i32) -> i32 {
    x & _WSTOPPED
}

/// _WSTATUS if process is stopped
pub const _WSTOPPED: i32 = 0o177;

pub const fn WIFSTOPPED(x: i32) -> bool {
    x == _WSTOPPED
}

pub const fn WSTOPSIG(x: i32) -> i32 {
    x >> 8
}

pub const fn WIFSIGNALED(x: i32) -> bool {
    _WSTATUS(x) != _WSTOPPED && _WSTATUS(x) != 0 && x != 0x13
}

#[inline]
pub const fn WTERMSIG(x: i32) -> i32 {
    _WSTATUS(x)
}

pub const fn WIFEXITED(x: i32) -> bool {
    _WSTATUS(x) == 0
}

pub const fn WEXITSTATUS(x: i32) -> i32 {
    x >> 8
}

pub const fn WIFCONTINUED(x: i32) -> bool {
    // 0x13 == SIGCONT
    x == 0x13
}

pub const fn WCOREDUMP(x: i32) -> i32 {
    x & WCOREFLAG
}

pub const fn W_EXITCODE(ret: i32, sig: i32) -> i32 {
    ret << 8 | sig
}

pub const fn W_STOPCODE(sig: i32) -> i32 {
    sig << 8 | _WSTOPPED
}

/*
 * Option bits for the third argument of wait4.  WNOHANG causes the
 * wait to not hang if there are no stopped or terminated processes, rather
 * returning an error indication in this case (pid==0).  WUNTRACED
 * indicates that the caller should receive status about untraced children
 * which stop due to signals.  If children are stopped and a wait without
 * this option is done, it is as though they were still running... nothing
 * about them is returned. WNOWAIT only request information about zombie,
 * leaving the proc around, available for later waits.
 */
/// Don't hang in wait.
pub const WNOHANG: i32 = 1;
/// Tell about stopped, untraced children.
pub const WUNTRACED: i32 = 2;
/// SUS compatibility
pub const WSTOPPED: i32 = WUNTRACED;
/// Report a job control continued process.
pub const WCONTINUED: i32 = 4;
/// Poll only. Don't delete the proc entry.
pub const WNOWAIT: i32 = 8;
/// Wait for exited processes.
pub const WEXITED: i32 = 16;
/// Wait for a process to hit a trap or a breakpoint.
pub const WTRAPPED: i32 = 32;

/// Wait for kthread spawned from linux_clone.
#[allow(overflowing_literals)]
pub const WLINUXCLONE: i32 = 0x80000000;

/// The type of id_t we are using.
///
/// These names were mostly lifted from Solaris source code and
/// still use Solaris style naming to avoid breaking any
/// OpenSolaris code which has been ported to FreeBSD.  There
/// is no clear FreeBSD counterpart for all of the names, but
/// some have a clear correspondence to FreeBSD entities.
///
/// The numerical values are kept synchronized with the Solaris values.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum idtype_t {
    /// A process identifier.
    P_PID,

    /// A parent process identifier.
    P_PPID,

    /// A process group identifier.
    P_PGID,

    /// A session identifier.
    P_SID,

    /// A scheduling class identifier.
    P_CID,

    /// A user identifier.
    P_UID,

    /// A group identifier.
    P_GID,

    /// All processes.
    P_ALL,

    /// An LWP identifier.
    P_LWPID,

    /// A task identifier.
    P_TASKID,

    /// A project identifier.
    P_PROJID,

    /// A pool identifier.
    P_POOLID,

    /// A zone identifier.
    P_JAILID,

    /// A (process) contract identifier.
    P_CTID,

    /// CPU identifier.
    P_CPUID,

    /// Processor set identifier.
    P_PSETID,
}

/// Tokens for special values of the "pid" parameter to wait4.
/// Extended struct __wrusage to collect rusage for both the target
/// process and its children within one wait6() call.
///
/// Any process
pub const WAIT_ANY: i32 = -1;

/// any process in my process group
pub const WAIT_MYPGRP: i32 = 0;
