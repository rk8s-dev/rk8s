// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/arm/include/uapi/asm/signal.h`

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
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_possible_wrap)]
pub const SIGRTMAX: i32 = _NSIG as i32;

pub const SIGSWI: i32 = 32;

/// `SA_THIRTYTWO` historically meant deliver the signal in 32-bit mode, even if
/// the task is running in 26-bit. But since the kernel no longer supports
/// 26-bit mode, the flag has no effect.
pub const SA_THIRTYTWO: usize = 0x0200_0000;
pub const SA_RESTORER: usize = 0x0400_0000;

pub const MINSIGSTKSZ: usize = 2048;
pub const SIGSTKSZ: usize = 8192;

/// Here we must cater to libcs that poke about in kernel headers.

pub type sa_sigaction_fn_t = fn(i32, &mut siginfo_t, usize);

/// `sa_sigaction_fn_t` as usize
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
        write!(f, "sigaction_u_t: {ptr}")
    }
}
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigaction_t {
    pub sa_handler: sighandler_t,
    pub sa_mask: sigset_t,
    pub sa_flags: usize,
    pub sa_restorer: sigrestore_t,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigaltstack_t {
    pub ss_sp: uintptr_t,
    pub ss_flags: i32,
    pub ss_size: size_t,
}

pub type stack_t = sigaltstack_t;
