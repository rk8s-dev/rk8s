// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/signal.h`

#![allow(clippy::module_name_repetitions)]

use super::{siginfo_t, sigset_t, sigval_t};

pub const _NSIG: i32 = 64;

pub const NSIG: i32 = _NSIG;

/// hangup
pub const SIGHUP: i32 = 1;
/// interrupt
pub const SIGINT: i32 = 2;
/// quit
pub const SIGQUIT: i32 = 3;
/// illegal instruction (not reset when caught)
pub const SIGILL: i32 = 4;
/// trace trap (not reset when caught)
pub const SIGTRAP: i32 = 5;
/// abort()
pub const SIGABRT: i32 = 6;
/// compatibility
pub const SIGIOT: i32 = SIGABRT;
/// EMT instruction
pub const SIGEMT: i32 = 7;
/// floating point exception
pub const SIGFPE: i32 = 8;
/// kill (cannot be caught or ignored)
pub const SIGKILL: i32 = 9;
/// bus error
pub const SIGBUS: i32 = 10;
/// segmentation violation
pub const SIGSEGV: i32 = 11;
/// bad argument to system call
pub const SIGSYS: i32 = 12;
/// write on a pipe with no one to read it
pub const SIGPIPE: i32 = 13;
/// alarm clock
pub const SIGALRM: i32 = 14;
/// software termination signal from kill
pub const SIGTERM: i32 = 15;
/// urgent condition on IO channel
pub const SIGURG: i32 = 16;
/// sendable stop signal not from tty
pub const SIGSTOP: i32 = 17;
/// stop signal from tty
pub const SIGTSTP: i32 = 18;
/// continue a stopped process
pub const SIGCONT: i32 = 19;
/// to parent on child stop or exit
pub const SIGCHLD: i32 = 20;
/// to readers pgrp upon background tty read
pub const SIGTTIN: i32 = 21;
/// like TTIN for output if (tp->t_local&LTOSTOP)
pub const SIGTTOU: i32 = 22;
/// input/output possible signal
pub const SIGIO: i32 = 23;
/// exceeded CPU time limit
pub const SIGXCPU: i32 = 24;
/// exceeded file size limit
pub const SIGXFSZ: i32 = 25;
/// virtual time alarm
pub const SIGVTALRM: i32 = 26;
/// profiling time alarm
pub const SIGPROF: i32 = 27;
/// window size changes
pub const SIGWINCH: i32 = 28;
/// information request
pub const SIGINFO: i32 = 29;
/// user defined signal 1
pub const SIGUSR1: i32 = 30;
/// user defined signal 2
pub const SIGUSR2: i32 = 31;
/// power fail/restart (not reset when caught)
pub const SIGPWR: i32 = 32;
pub const SIGRTMIN: i32 = 33;
pub const SIGRTMAX: i32 = 63;

/// Type of a signal handler.
///
/// `sighandler_fn` as usize
pub type sighandler_t = usize;

pub const SIG_DFL: sighandler_t = 0;
pub const SIG_IGN: sighandler_t = 1;
#[allow(clippy::cast_sign_loss)]
pub const SIG_ERR: sighandler_t = -1_isize as sighandler_t;
pub const SIG_HOLD: sighandler_t = 3;

pub type sighandler_fn = fn(i32);
pub type sigaction_fn = fn(i32, &mut siginfo_t, usize);

#[repr(C)]
pub union sa_union {
    _sa_handler: sighandler_fn,
    _sa_sigaction: sigaction_fn,
}

/// Signal vector "template" used in sigaction call.
#[repr(C)]
pub struct sigaction_t {
    /// signal handler
    pub _sa_u: sa_union,
    /// signal mask to apply
    pub sa_mask: sigset_t,
    /// see signal options below
    sa_flags: i32,
}

/// take signal on signal stack
pub const SA_ONSTACK: i32 = 0x0001;
/// restart system call on signal return
pub const SA_RESTART: i32 = 0x0002;
/// reset to `SIG_DFL` when taking signal
pub const SA_RESETHAND: i32 = 0x0004;
/// don't mask the signal we're delivering
pub const SA_NODEFER: i32 = 0x0010;
/// Only valid for SIGCHLD.
/// do not generate SIGCHLD on child stop
pub const SA_NOCLDSTOP: i32 = 0x0008;
/// do not generate zombies on unwaited child
pub const SA_NOCLDWAIT: i32 = 0x0020;
/// take `sa_sigaction` handler
pub const SA_SIGINFO: i32 = 0x0040;
/// siginfo does not print kernel info on tty
pub const SA_NOKERNINFO: i32 = 0x0080;
pub const SA_ALLBITS: i32 = 0x00ff;

/// Flags for `sigprocmask()`:
///
/// block specified signal set
pub const SIG_BLOCK: i32 = 1;
/// unblock specified signal set
pub const SIG_UNBLOCK: i32 = 2;
/// set specified signal set
pub const SIG_SETMASK: i32 = 3;

/// type of signal function
pub type signal_fn = fn(i32);
pub type sig_t = signal_fn;

/// Flags used with `stack_t/struct` sigaltstack.
///
/// take signals on alternate stack
pub const SS_ONSTACK: i32 = 0x0001;
/// disable taking signals on alternate stack
pub const SS_DISABLE: i32 = 0x0004;
pub const SS_ALLBITS: i32 = 0x0005;
/// minimum allowable stack
pub const MINSIGSTKSZ: i32 = 8192;
/// recommended stack size
pub const SIGSTKSZ: i32 = MINSIGSTKSZ + 32768;

/// Structure used in sigstack call.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigstack_t {
    /// signal stack pointer
    pub ss_sp: usize,
    /// current status
    pub ss_onstack: i32,
}

pub const BADSIG: sighandler_t = SIG_ERR;

pub type sigev_notify_fn = fn(sigval_t);

#[repr(C)]
#[derive(Debug, Clone)]
pub struct sigevent_t {
    pub sigev_notify: i32,
    pub sigev_signo: i32,
    pub sigev_value: sigval_t,
    pub sigev_notify_function: sigev_notify_fn,
    /// pthread_attr_t
    pub sigev_notify_attributes: usize,
}

pub const SIGEV_NONE: i32 = 0;
pub const SIGEV_SIGNAL: i32 = 1;
pub const SIGEV_THREAD: i32 = 2;
pub const SIGEV_SA: i32 = 3;
