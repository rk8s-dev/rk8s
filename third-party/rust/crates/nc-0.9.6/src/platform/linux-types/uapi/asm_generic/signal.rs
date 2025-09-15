// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/signal.h`

use crate::{sighandler_t, sigrestore_t, size_t, BITS_PER_LONG};

pub const _NSIG: usize = 64;
pub const _NSIG_BPW: usize = BITS_PER_LONG;
pub const _NSIG_WORDS: usize = _NSIG / _NSIG_BPW;

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
//pub const SIGLOST: i32 = 29;
pub const SIGPWR: i32 = 30;
pub const SIGSYS: i32 = 31;
pub const SIGUNUSED: i32 = 31;

/// These should not be considered constants from userland.
pub const SIGRTMIN: i32 = 32;
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_possible_wrap)]
pub const SIGRTMAX: i32 = _NSIG as i32;

#[cfg(not(target_arch = "aarch64"))]
pub const MINSIGSTKSZ: usize = 2048;
#[cfg(not(target_arch = "aarch64"))]
pub const SIGSTKSZ: usize = 8192;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigset_t {
    pub sig: [usize; _NSIG_WORDS],
}

/// not actually used, but required for linux/syscalls.h
pub type old_sigset_t = usize;

impl From<old_sigset_t> for sigset_t {
    fn from(val: old_sigset_t) -> Self {
        let mut s = Self::default();
        s.sig[0] = val;
        s
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigaction_t {
    pub sa_handler: sighandler_t,
    pub sa_flags: usize,

    // NOTE(Shaohua): riscv64 does not have SA_RESTORER
    #[cfg(nc_has_sa_restorer)]
    pub sa_restorer: sigrestore_t,

    /// mask last for extensibility
    pub sa_mask: sigset_t,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigaltstack_t {
    pub ss_sp: usize,
    pub ss_flags: i32,
    pub ss_size: size_t,
}

pub type stack_t = sigaltstack_t;
