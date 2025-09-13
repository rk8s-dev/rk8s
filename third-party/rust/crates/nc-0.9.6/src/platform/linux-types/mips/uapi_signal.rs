// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/mips/include/uapi/asm/signal.h`

use core::mem::size_of;

use crate::{sighandler_t, size_t, uintptr_t};

pub const _NSIG: usize = 128;
pub const _NSIG_BPW: usize = size_of::<usize>() * 8;
pub const _NSIG_WORDS: usize = _NSIG / _NSIG_BPW;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct sigset_t {
    pub sig: [usize; _NSIG_WORDS],
}

// at least 32 bits
pub type old_sigset_t = usize;

impl From<old_sigset_t> for sigset_t {
    fn from(val: old_sigset_t) -> Self {
        let mut s = Self::default();
        s.sig[0] = val;
        s
    }
}

/// Hangup (POSIX).
pub const SIGHUP: i32 = 1;
/// Interrupt (ANSI).
pub const SIGINT: i32 = 2;
/// Quit (POSIX).
pub const SIGQUIT: i32 = 3;
/// Illegal instruction (ANSI).
pub const SIGILL: i32 = 4;
/// Trace trap (POSIX).
pub const SIGTRAP: i32 = 5;
/// IOT trap (4.2 BSD).
pub const SIGIOT: i32 = 6;
/// Abort (ANSI).
pub const SIGABRT: i32 = SIGIOT;
pub const SIGEMT: i32 = 7;
/// Floating-point exception (ANSI).
pub const SIGFPE: i32 = 8;
/// Kill, unblockable (POSIX).
pub const SIGKILL: i32 = 9;
/// BUS error (4.2 BSD).
pub const SIGBUS: i32 = 10;
/// Segmentation violation (ANSI).
pub const SIGSEGV: i32 = 11;
pub const SIGSYS: i32 = 12;
/// Broken pipe (POSIX).
pub const SIGPIPE: i32 = 13;
/// Alarm clock (POSIX).
pub const SIGALRM: i32 = 14;
/// Termination (ANSI).
pub const SIGTERM: i32 = 15;
/// User-defined signal 1 (POSIX).
pub const SIGUSR1: i32 = 16;
/// User-defined signal 2 (POSIX).
pub const SIGUSR2: i32 = 17;
/// Child status has changed (POSIX).
pub const SIGCHLD: i32 = 18;
/// Same as SIGCHLD (System V).
pub const SIGCLD: i32 = SIGCHLD;
/// Power failure restart (System V).
pub const SIGPWR: i32 = 19;
/// Window size change (4.3 BSD, Sun).
pub const SIGWINCH: i32 = 20;
/// Urgent condition on socket (4.2 BSD).
pub const SIGURG: i32 = 21;
/// I/O now possible (4.2 BSD).
pub const SIGIO: i32 = 22;
/// Pollable event occurred (System V).
pub const SIGPOLL: i32 = SIGIO;
/// Stop, unblockable (POSIX).
pub const SIGSTOP: i32 = 23;
/// Keyboard stop (POSIX).
pub const SIGTSTP: i32 = 24;
/// Continue (POSIX).
pub const SIGCONT: i32 = 25;
/// Background read from tty (POSIX).
pub const SIGTTIN: i32 = 26;
/// Background write to tty (POSIX).
pub const SIGTTOU: i32 = 27;
/// Virtual alarm clock (4.2 BSD).
pub const SIGVTALRM: i32 = 28;
/// Profiling alarm clock (4.2 BSD).
pub const SIGPROF: i32 = 29;
/// CPU limit exceeded (4.2 BSD).
pub const SIGXCPU: i32 = 30;
/// File size limit exceeded (4.2 BSD).
pub const SIGXFSZ: i32 = 31;

/// These should not be considered constants from userland.
pub const SIGRTMIN: i32 = 32;
pub const SIGRTMAX: i32 = _NSIG as i32;

/*
 * SA_RESTORER used to be defined as 0x04000000 but only the O32 ABI ever
 * supported its use and no libc was using it, so the entire sa-restorer
 * functionality was removed with lmo commit 39bffc12c3580ab for 2.5.48
 * retaining only the SA_RESTORER definition as a reminder to avoid
 * accidental reuse of the mask bit.
 */
pub const SA_ONSTACK: u32 = 0x0800_0000;
pub const SA_RESETHAND: u32 = 0x8000_0000;
pub const SA_RESTART: u32 = 0x1000_0000;
pub const SA_SIGINFO: u32 = 0x0000_0008;
pub const SA_NODEFER: u32 = 0x4000_0000;
pub const SA_NOCLDWAIT: u32 = 0x0001_0000;
pub const SA_NOCLDSTOP: u32 = 0x0000_0001;

pub const SA_NOMASK: u32 = SA_NODEFER;
pub const SA_ONESHOT: u32 = SA_RESETHAND;

pub const MINSIGSTKSZ: usize = 2048;
pub const SIGSTKSZ: usize = 8192;

/// for blocking signals
pub const SIG_BLOCK: i32 = 1;
/// for unblocking signals
pub const SIG_UNBLOCK: i32 = 2;
/// for setting the signal mask
pub const SIG_SETMASK: i32 = 3;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigaction_t {
    pub sa_flags: u32,
    pub sa_handler: sighandler_t,
    pub sa_mask: sigset_t,
}

/// IRIX compatible stack_t
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigaltstack_t {
    pub ss_sp: uintptr_t,
    pub ss_size: size_t,
    pub ss_flags: i32,
}
pub type stack_t = sigaltstack_t;
