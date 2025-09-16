// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/signal.h`

use core::ffi::c_void;
use core::fmt;
use core::ptr::null_mut;

use crate::{pid_t, sigset_t, uid_t};

/// counting 0; could be 33 (mask is 1-32)
pub const __DARWIN_NSIG: i32 = 32;

pub const NSIG: i32 = __DARWIN_NSIG;

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

/// Type of a signal handler.
///
/// `sighandler_fn` as usize
pub type sighandler_t = usize;

pub const SIG_DFL: sighandler_t = 0;
pub const SIG_IGN: sighandler_t = 1;
pub const SIG_HOLD: sighandler_t = 5;
#[allow(clippy::cast_sign_loss)]
pub const SIG_ERR: sighandler_t = -1_isize as sighandler_t;

#[repr(C)]
#[derive(Clone, Copy)]
pub union sigval_u {
    /// Members as suggested by Annex C of POSIX 1003.1b.
    sival_int: i32,
    sival_ptr: *mut c_void,
}

impl Default for sigval_u {
    fn default() -> Self {
        Self { sival_int: 0 }
    }
}

impl fmt::Debug for sigval_u {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let flags = unsafe { self.sival_int };
        write!(f, "sival_int: {}", flags)
    }
}

/// No async notification
pub const SIGEV_NONE: i32 = 0;
/// aio - completion notification
pub const SIGEV_SIGNAL: i32 = 1;
/// [NOTIMP] [RTS] call notification function
pub const SIGEV_THREAD: i32 = 3;

/// Notification function
pub type sigev_notify_fn = fn(sigval_u);

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct sigevent_t {
    /// Notification type
    pub sigev_notify: i32,
    /// Signal number
    pub sigev_signo: i32,
    /// Signal value
    pub sigev_value: sigval_u,
    /// Notification function
    pub sigev_notify_function: sigev_notify_fn,
    /// Notification attributes
    // TODO(Shaohua): Add pthread_attr_t
    //pthread_attr_t *sigev_notify_attributes;
    pub sigev_notify_attributes: *mut c_void,
}

impl Default for sigevent_t {
    fn default() -> Self {
        Self {
            sigev_notify: 0,
            sigev_signo: 0,
            sigev_value: sigval_u::default(),
            sigev_notify_function: unsafe {
                core::mem::transmute::<*mut c_void, sigev_notify_fn>(null_mut())
            },
            sigev_notify_attributes: null_mut(),
        }
    }
}

#[repr(C)]
pub struct siginfo_t {
    /// signal number
    pub si_signo: i32,
    /// errno association
    pub si_errno: i32,
    /// signal code
    pub si_code: i32,
    /// sending process
    pub si_pid: pid_t,
    /// sender's ruid
    pub si_uid: uid_t,
    /// exit value
    pub si_status: i32,
    /// faulting instruction
    pub si_addr: *mut c_void,
    /// signal value
    pub si_value: sigval_u,
    /// band event for SIGPOLL
    pub si_band: isize,
    /// Reserved for Future Use
    __pad: [usize; 7],
}

/// Values for `si_code`
///
/// Codes for SIGILL
/// if only I knew...
pub const ILL_NOOP: i32 = 0;
/// [XSI] illegal opcode
pub const ILL_ILLOPC: i32 = 1;
/// [XSI] illegal trap
pub const ILL_ILLTRP: i32 = 2;
/// [XSI] privileged opcode
pub const ILL_PRVOPC: i32 = 3;
/// [XSI] illegal operand -NOTIMP
pub const ILL_ILLOPN: i32 = 4;
/// [XSI] illegal addressing mode -NOTIMP
pub const ILL_ILLADR: i32 = 5;
/// [XSI] privileged register -NOTIMP
pub const ILL_PRVREG: i32 = 6;
/// [XSI] coprocessor error -NOTIMP
pub const ILL_COPROC: i32 = 7;
/// [XSI] internal stack error -NOTIMP
pub const ILL_BADSTK: i32 = 8;

/// Codes for SIGFPE
///
/// if only I knew...
pub const FPE_NOOP: i32 = 0;
/// [XSI] floating point divide by zero
pub const FPE_FLTDIV: i32 = 1;
/// [XSI] floating point overflow
pub const FPE_FLTOVF: i32 = 2;
/// [XSI] floating point underflow
pub const FPE_FLTUND: i32 = 3;
/// [XSI] floating point inexact result
pub const FPE_FLTRES: i32 = 4;
/// [XSI] invalid floating point operation
pub const FPE_FLTINV: i32 = 5;
/// [XSI] subscript out of range -NOTIMP
pub const FPE_FLTSUB: i32 = 6;
/// [XSI] integer divide by zero
pub const FPE_INTDIV: i32 = 7;
/// [XSI] integer overflow
pub const FPE_INTOVF: i32 = 8;

/// Codes for SIGSEGV
/// if only I knew...
pub const SEGV_NOOP: i32 = 0;
/// [XSI] address not mapped to object
pub const SEGV_MAPERR: i32 = 1;
/// [XSI] invalid permission for mapped object
pub const SEGV_ACCERR: i32 = 2;

/// Codes for SIGBUS
/// if only I knew...
pub const BUS_NOOP: i32 = 0;
/// [XSI] Invalid address alignment
pub const BUS_ADRALN: i32 = 1;
/// [XSI] Nonexistent physical address -NOTIMP
pub const BUS_ADRERR: i32 = 2;
/// [XSI] Object-specific HW error - NOTIMP
pub const BUS_OBJERR: i32 = 3;

/// Codes for SIGTRAP
/// [XSI] Process breakpoint -NOTIMP
pub const TRAP_BRKPT: i32 = 1;
/// [XSI] Process trace trap -NOTIMP
pub const TRAP_TRACE: i32 = 2;

/// Codes for SIGCHLD
/// if only I knew...
pub const CLD_NOOP: i32 = 0;
/// [XSI] child has exited
pub const CLD_EXITED: i32 = 1;
/// [XSI] terminated abnormally, no core file
pub const CLD_KILLED: i32 = 2;
/// [XSI] terminated abnormally, core file
pub const CLD_DUMPED: i32 = 3;
/// [XSI] traced child has trapped
pub const CLD_TRAPPED: i32 = 4;
/// [XSI] child has stopped
pub const CLD_STOPPED: i32 = 5;
/// [XSI] stopped child has continued
pub const CLD_CONTINUED: i32 = 6;

/// Codes for SIGPOLL
/// [XSR] Data input available
pub const POLL_IN: i32 = 1;
/// [XSR] Output buffers available
pub const POLL_OUT: i32 = 2;
/// [XSR] Input message available
pub const POLL_MSG: i32 = 3;
/// [XSR] I/O error
pub const POLL_ERR: i32 = 4;
/// [XSR] High priority input available
pub const POLL_PRI: i32 = 5;
/// [XSR] Device disconnected
pub const POLL_HUP: i32 = 6;

pub type sa_handler_fn = fn(i32);
pub type sa_sigaction_fn = fn(i32, *mut siginfo_t, *mut c_void);

/// union for signal handlers
#[repr(C)]
pub union sigaction_u {
    sa_handler: sa_handler_fn,
    sa_sigaction: sa_sigaction_fn,
}

pub type sa_tramp_fn = fn(*mut c_void, i32, i32, *mut siginfo_t, *mut c_void);

/// Signal vector template for Kernel user boundary
#[repr(C)]
pub struct __sigaction_t {
    /// signal handler
    pub __sigaction_u: sigaction_u,
    pub sa_tramp: sa_tramp_fn,
    /// signal mask to apply
    pub sa_mask: sigset_t,
    /// see signal options below
    pub sa_flags: i32,
}

/// Signal vector "template" used in sigaction call.
#[repr(C)]
pub struct sigaction_t {
    /// signal handler
    pub __sigaction_u: sigaction_u,
    /// signal mask to apply
    pub sa_mask: sigset_t,
    /// see signal options below
    pub sa_flags: i32,
}

/// take signal on signal stack
pub const SA_ONSTACK: i32 = 0x0001;
/// restart system on signal return
pub const SA_RESTART: i32 = 0x0002;
/// reset to `SIG_DFL` when taking signal
pub const SA_RESETHAND: i32 = 0x0004;
/// do not generate SIGCHLD on child stop
pub const SA_NOCLDSTOP: i32 = 0x0008;
/// don't mask the signal we're delivering
pub const SA_NODEFER: i32 = 0x0010;
/// don't keep zombies around
pub const SA_NOCLDWAIT: i32 = 0x0020;
/// signal handler with `SA_SIGINFO` args
pub const SA_SIGINFO: i32 = 0x0040;
/// do not bounce off kernel's sigtramp
pub const SA_USERTRAMP: i32 = 0x0100;
/// This will provide 64bit register set in a 32bit user address space
/// signal handler with `SA_SIGINFO` args with 64bit regs information
pub const SA_64REGSET: i32 = 0x0200;

/// the following are the only bits we support from user space, the
/// rest are for kernel use only.
pub const SA_USERSPACE_MASK: i32 =
    SA_ONSTACK | SA_RESTART | SA_RESETHAND | SA_NOCLDSTOP | SA_NODEFER | SA_NOCLDWAIT | SA_SIGINFO;

/// Flags for sigprocmask:
///
/// block specified signal set
pub const SIG_BLOCK: i32 = 1;
/// unblock specified signal set
pub const SIG_UNBLOCK: i32 = 2;
/// set specified signal set
pub const SIG_SETMASK: i32 = 3;

/// POSIX 1003.1b required values.
/// [CX] signal from kill()
pub const SI_USER: i32 = 0x10001;
/// [CX] signal from sigqueue()
pub const SI_QUEUE: i32 = 0x10002;
/// [CX] timer expiration
pub const SI_TIMER: i32 = 0x10003;
/// [CX] aio request completion
pub const SI_ASYNCIO: i32 = 0x10004;
/// [CX]from message arrival on empty queue
pub const SI_MESGQ: i32 = 0x10005;

/// type of signal function
pub type sig_fn = fn(i32);

/// Structure used in sigaltstack call.
///
/// take signal on signal stack
pub const SS_ONSTACK: i32 = 0x0001;
/// disable taking signals on alternate stack
pub const SS_DISABLE: i32 = 0x0004;
/// (32K)minimum allowable stack
pub const MINSIGSTKSZ: i32 = 32768;
/// (128K)recommended stack size
pub const SIGSTKSZ: i32 = 131_072;

/// 4.3 compatibility:
/// Signal vector "template" used in sigvec call.
#[repr(C)]
pub struct sigvec_t {
    /// signal handler
    pub sv_handler: sa_handler_fn,
    /// signal mask to apply
    pub sv_mask: i32,
    /// see signal options below
    pub sv_flags: i32,
}

pub const SV_ONSTACK: i32 = SA_ONSTACK;
/// same bit, opposite sense
pub const SV_INTERRUPT: i32 = SA_RESTART;
pub const SV_RESETHAND: i32 = SA_RESETHAND;
pub const SV_NODEFER: i32 = SA_NODEFER;
pub const SV_NOCLDSTOP: i32 = SA_NOCLDSTOP;
pub const SV_SIGINFO: i32 = SA_SIGINFO;

/// Structure used in sigstack call.
#[repr(C)]
pub struct sigstack_t {
    /// signal stack pointer
    pub ss_sp: *mut c_void,
    /// current status
    pub ss_onstack: i32,
}

/// Converting signal number to a mask suitable for sigblock().
#[must_use]
pub const fn sigmask(m: i32) -> i32 {
    1 << (m - 1)
}

pub const BADSIG: usize = SIG_ERR;
