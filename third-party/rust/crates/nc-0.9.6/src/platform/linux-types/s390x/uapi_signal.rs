// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/s390/include/uapi/asm/signal.h`
//!
//! S390 version
//! Derived from "include/asm-i386/signal.h"

use core::fmt;

use crate::{sighandler_t, siginfo_t, sigrestore_t, sigset_t, size_t, uintptr_t, SIG_DFL, _NSIG};

pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGILL: i32 = 4;
pub const SIGTRAP: i32 = 5;
pub const SIGABRT: i32 = 6;
pub const SIGIOT: i32 = 6;
pub const SIGBUS: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGKILL: i32 = 9;
pub const SIGUSR1: i32 = 10;
pub const SIGSEGV: i32 = 11;
pub const SIGUSR2: i32 = 12;
pub const SIGPIPE: i32 = 13;
pub const SIGALRM: i32 = 14;
pub const SIGTERM: i32 = 15;
pub const SIGSTKFLT: i32 = 16;
pub const SIGCHLD: i32 = 17;
pub const SIGCONT: i32 = 18;
pub const SIGSTOP: i32 = 19;
pub const SIGTSTP: i32 = 20;
pub const SIGTTIN: i32 = 21;
pub const SIGTTOU: i32 = 22;
pub const SIGURG: i32 = 23;
pub const SIGXCPU: i32 = 24;
pub const SIGXFSZ: i32 = 25;
pub const SIGVTALRM: i32 = 26;
pub const SIGPROF: i32 = 27;
pub const SIGWINCH: i32 = 28;
pub const SIGIO: i32 = 29;
pub const SIGPOLL: i32 = SIGIO;
// pub const SIGLOST: i32 = 29;
pub const SIGPWR: i32 = 30;
pub const SIGSYS: i32 = 31;
pub const SIGUNUSED: i32 = 31;

/// These should not be considered constants from userland.
pub const SIGRTMIN: i32 = 32;
pub const SIGRTMAX: i32 = _NSIG as i32;

pub const SA_RESTORER: usize = 0x04000000;

pub const MINSIGSTKSZ: usize = 2048;
pub const SIGSTKSZ: usize = 8192;

/// There are two system calls in regard to sigaction, sys_rt_sigaction
/// and sys_sigaction. Internally the kernel uses the struct old_sigaction
/// for the older sys_sigaction system call, and the kernel version of the
/// struct sigaction for the newer sys_rt_sigaction.
///
/// The uapi definition for struct sigaction has made a strange distinction
/// between 31-bit and 64-bit in the past. For 64-bit the uapi structure
/// looks like the kernel struct sigaction, but for 31-bit it used to
/// look like the kernel struct old_sigaction. That practically made the
/// structure unusable for either system call. To get around this problem
/// the glibc always had its own definitions for the sigaction structures.
///
/// The current struct sigaction uapi definition below is suitable for the
/// sys_rt_sigaction system call only.

pub type sa_sigaction_fn_t = fn(i32, &mut siginfo_t, usize);

/// sa_sigaction_fn_t as usize
pub type sa_sigaction_t = usize;

#[repr(C)]
#[derive(Clone, Copy)]
pub union sigaction_u_t {
    pub sa_handler: sighandler_t,
    pub sa_sigaction: sa_sigaction_t,
}

impl Default for sigaction_u_t {
    fn default() -> Self {
        sigaction_u_t {
            sa_handler: SIG_DFL,
        }
    }
}

impl fmt::Debug for sigaction_u_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ptr = unsafe { self.sa_handler };
        write!(f, "sigaction_u_t: {}", ptr)
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigaction_t {
    /// Actually its type is `sigaction_u_t`.
    /// Keep synched with rust-lang/libc.
    pub sa_handler: sighandler_t,
    pub sa_flags: usize,
    pub sa_restorer: sigrestore_t,
    pub sa_mask: sigset_t,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigaltstack_t {
    pub ss_sp: uintptr_t,
    pub ss_flags: i32,
    pub ss_size: size_t,
}

pub type stack_t = sigaltstack_t;
