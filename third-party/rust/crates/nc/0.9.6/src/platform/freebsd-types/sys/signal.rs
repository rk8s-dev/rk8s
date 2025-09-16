// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! from `sys/signal.h`

use core::fmt;

use crate::{lwpid_t, pid_t, sigset_t, size_t, uid_t, __MINSIGSTKSZ};

/// System defined signals.
/// hangup
pub const SIGHUP: i32 = 1;
/// interrupt
pub const SIGINT: i32 = 2;
/// quit
pub const SIGQUIT: i32 = 3;
/// illegal instr. (not reset when caught)
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
/// non-existent system call invoked
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
/// like TTIN if `(tp->t_local&LTOSTOP)`
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
/// reserved by thread library.
pub const SIGTHR: i32 = 32;
pub const SIGLWP: i32 = SIGTHR;
/// reserved by real-time library.
pub const SIGLIBRT: i32 = 33;

pub const SIGRTMIN: i32 = 65;
pub const SIGRTMAX: i32 = 126;

pub const SIG_DFL: sighandler_t = 0;
pub const SIG_IGN: sighandler_t = 1;
#[allow(clippy::cast_sign_loss)]
pub const SIG_ERR: sighandler_t = -1_isize as sighandler_t;
// #define	SIG_CATCH	((__sighandler_t *)2) See signalvar.h
pub const SIG_HOLD: sighandler_t = 3;

/// Type of a signal handling function.
///
/// Language spec sez signal handlers take exactly one arg, even though we
/// actually supply three.  Ugh!
///
/// We don't try to hide the difference by leaving out the args because
/// that would cause warnings about conformant programs.  Nonconformant
/// programs can avoid the warnings by casting to `(__sighandler_t *)` or
/// `sig_t` before calling `signal()` or assigning to `sa_handler` or `sv_handler`.
///
/// The kernel should reverse the cast before calling the function.  It
/// has no way to do this, but on most machines 1-arg and 3-arg functions
/// have the same calling protocol so there is no problem in practice.
/// A bit in `sa_flags` could be used to specify the number of args.
pub type sighandler_t = usize;

#[repr(C)]
#[derive(Clone, Copy)]
pub union sigval_u_t {
    /// Members as suggested by Annex C of POSIX 1003.1b.
    pub sival_int: i32,
    pub sival_ptr: usize,
    /// 6.0 compatibility
    pub sigval_int: i32,
    pub sigval_ptr: usize,
}

impl Default for sigval_u_t {
    fn default() -> Self {
        Self { sigval_ptr: 0 }
    }
}

impl fmt::Debug for sigval_u_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ptr = unsafe { self.sigval_ptr };
        write!(f, "sigval_ptr: {}", ptr)
    }
}

#[cfg(target_pointer_width = "64")]
#[repr(C)]
#[derive(Clone, Copy)]
pub union sigval32_t {
    pub sival_int: i32,
    pub sival_ptr: u32,
    /// 6.0 compatibility
    pub sigval_int: i32,
    pub sigval_ptr: u32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct sigev_thread_t {
    /// void (*_function)(union sigval);
    pub function: usize,

    /// struct pthread_attr **_attribute;
    pub attribute: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union sigev_un_t {
    pub threadid: lwpid_t,
    pub sigev_thread: sigev_thread_t,
    pub kevent_flags: u16,
    pub __spare__: [isize; 8],
}

impl Default for sigev_un_t {
    fn default() -> Self {
        Self { kevent_flags: 0 }
    }
}

impl fmt::Debug for sigev_un_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let flags = unsafe { self.kevent_flags };
        write!(f, "_kevent_flags: {}", flags)
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct sigevent_t {
    /// Notification type
    pub sigev_notify: i32,
    /// Signal number
    pub sigev_signo: i32,
    /// Signal value
    pub sigev_value: sigval_u_t,
    pub sigev_un: sigev_un_t,
}

/// No async notification.
pub const SIGEV_NONE: i32 = 0;
/// Generate a queued signal.
pub const SIGEV_SIGNAL: i32 = 1;
/// Call back from another pthread.
pub const SIGEV_THREAD: i32 = 2;
/// Generate a kevent.
pub const SIGEV_KEVENT: i32 = 3;
/// Send signal to a kernel thread.
pub const SIGEV_THREAD_ID: i32 = 4;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct siginfo_reason_fault_t {
    /// machine specific trap code
    pub trapno: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct siginfo_reason_timer_t {
    pub timerid: i32,
    pub overrun: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct siginfo_reason_mesgq_t {
    pub mqd: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct siginfo_reason_poll_t {
    /// band event for SIGPOLL
    pub band: isize,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct siginfo_reason_capsicum_t {
    /// Syscall number for signals delivered as a result of system calls denied by Capsicum.
    pub syscall: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct siginfo_reason_spare_t {
    __spare1__: isize,
    __spare2__: [i32; 7],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union siginfo_reason_u_t {
    pub fault: siginfo_reason_fault_t,
    pub timer: siginfo_reason_timer_t,
    pub poll: siginfo_reason_poll_t,
    pub mesgq: siginfo_reason_mesgq_t,
    pub capsicum: siginfo_reason_capsicum_t,
    __spare__: siginfo_reason_spare_t,
}

impl Default for siginfo_reason_u_t {
    fn default() -> Self {
        Self {
            __spare__: siginfo_reason_spare_t::default(),
        }
    }
}

impl fmt::Debug for siginfo_reason_u_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let poll = unsafe { self.poll };
        write!(f, "poll: {:?}", poll)
    }
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct siginfo_t {
    /// signal number
    pub si_signo: i32,
    /// errno association
    pub si_errno: i32,
    /// Cause of signal, one of the SI_ macros or signal-specific
    /// values, i.e. one of the FPE_... values for SIGFPE.  This
    /// value is equivalent to the second argument to an old-style
    /// FreeBSD signal handler.
    /// signal code
    pub si_code: i32,
    /// sending process
    pub si_pid: pid_t,
    /// sender's ruid
    pub si_uid: uid_t,
    /// exit value
    pub si_status: i32,
    /// faulting instruction
    pub si_addr: usize,
    /// signal value
    pub si_value: sigval_u_t,
    pub reason: siginfo_reason_u_t,
}

/*
struct siginfo32 {
    int	si_signo;		/* signal number */
    int	si_errno;		/* errno association */
    int	si_code;		/* signal code */
    __pid_t	si_pid;			/* sending process */
    __uid_t	si_uid;			/* sender's ruid */
    int	si_status;		/* exit value */
    uint32_t si_addr;		/* faulting instruction */
    union sigval32 si_value;	/* signal value */
    union	{
        struct {
            int	_trapno;/* machine specific trap code */
        } _fault;
        struct {
            int	_timerid;
            int	_overrun;
        } _timer;
        struct {
            int	_mqd;
        } _mesgq;
        struct {
            int32_t	_band;		/* band event for SIGPOLL */
        } _poll;			/* was this ever used ? */
        struct {
            int32_t	__spare1__;
            int	__spare2__[7];
        } __spare__;
    } _reason;
};
*/

/// * `si_code` *
/// codes for SIGILL
/// Illegal opcode.
pub const ILL_ILLOPC: i32 = 1;
/// Illegal operand.
pub const ILL_ILLOPN: i32 = 2;
/// Illegal addressing mode.
pub const ILL_ILLADR: i32 = 3;
/// Illegal trap.
pub const ILL_ILLTRP: i32 = 4;
/// Privileged opcode.
pub const ILL_PRVOPC: i32 = 5;
/// Privileged register.
pub const ILL_PRVREG: i32 = 6;
/// Coprocessor error.
pub const ILL_COPROC: i32 = 7;
/// Internal stack error.
pub const ILL_BADSTK: i32 = 8;

/// codes for SIGBUS
/// Invalid address alignment.
pub const BUS_ADRALN: i32 = 1;
/// Nonexistent physical address.
pub const BUS_ADRERR: i32 = 2;
/// Object-specific hardware error.
pub const BUS_OBJERR: i32 = 3;
/// Non-standard: No memory.
pub const BUS_OOMERR: i32 = 100;

/// codes for SIGSEGV
/// Address not mapped to object.
pub const SEGV_MAPERR: i32 = 1;
/// Invalid permissions for mapped
/// object.
pub const SEGV_ACCERR: i32 = 2;
/// x86: PKU violation
pub const SEGV_PKUERR: i32 = 100;

/// codes for SIGFPE
/// Integer overflow.
pub const FPE_INTOVF: i32 = 1;
/// Integer divide by zero.
pub const FPE_INTDIV: i32 = 2;
/// Floating point divide by zero.
pub const FPE_FLTDIV: i32 = 3;
/// Floating point overflow.
pub const FPE_FLTOVF: i32 = 4;
/// Floating point underflow.
pub const FPE_FLTUND: i32 = 5;
/// Floating point inexact result.
pub const FPE_FLTRES: i32 = 6;
/// Invalid floating point operation.
pub const FPE_FLTINV: i32 = 7;
/// Subscript out of range.
pub const FPE_FLTSUB: i32 = 8;

/// codes for SIGTRAP
/// Process breakpoint.
pub const TRAP_BRKPT: i32 = 1;
/// Process trace trap.
pub const TRAP_TRACE: i32 = 2;
/// `DTrace` induced trap.
pub const TRAP_DTRACE: i32 = 3;
/// Capabilities protective trap.
pub const TRAP_CAP: i32 = 4;

/// codes for SIGCHLD
/// Child has exited
pub const CLD_EXITED: i32 = 1;
/// Child has terminated abnormally but
/// did not create a core file
pub const CLD_KILLED: i32 = 2;
/// Child has terminated abnormally and
/// created a core file
pub const CLD_DUMPED: i32 = 3;
/// Traced child has trapped
pub const CLD_TRAPPED: i32 = 4;
/// Child has stopped
pub const CLD_STOPPED: i32 = 5;
/// Stopped child has continued
pub const CLD_CONTINUED: i32 = 6;

/// codes for SIGPOLL
/// Data input available
pub const POLL_IN: i32 = 1;
/// Output buffers available
pub const POLL_OUT: i32 = 2;
/// Input message available
pub const POLL_MSG: i32 = 3;
/// I/O Error
pub const POLL_ERR: i32 = 4;
/// High priority input available
pub const POLL_PRI: i32 = 5;
/// Device disconnected
pub const POLL_HUP: i32 = 6;

#[repr(C)]
pub union sigaction_u_t {
    pub sa_handler: sighandler_t,
    pub sa_sigaction: siginfohandler_t,
}

impl Default for sigaction_u_t {
    fn default() -> Self {
        Self {
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

/// Signal vector "template" used in sigaction call.
#[repr(C)]
#[derive(Debug, Default)]
pub struct sigaction_t {
    /// signal handler
    pub u: sigaction_u_t,
    /// see signal options below
    pub sa_flags: i32,
    /// signal mask to apply
    pub sa_mask: sigset_t,
}

/// do not generate SIGCHLD on child stop
pub const SA_NOCLDSTOP: i32 = 0x0008;

/// take signal on signal stack
pub const SA_ONSTACK: i32 = 0x0001;
/// restart system call on signal return
pub const SA_RESTART: i32 = 0x0002;
/// reset to `SIG_DFL` when taking signal
pub const SA_RESETHAND: i32 = 0x0004;
/// don't mask the signal we're delivering
pub const SA_NODEFER: i32 = 0x0010;
/// don't keep zombies around
pub const SA_NOCLDWAIT: i32 = 0x0020;
/// signal handler with `SA_SIGINFO` args
pub const SA_SIGINFO: i32 = 0x0040;

/// number of old signals (counting 0)
pub const NSIG: i32 = 32;

/// No signal info besides `si_signo`.
pub const SI_NOINFO: i32 = 0;
/// Signal sent by kill().
pub const SI_USER: i32 = 0x10001;
/// Signal sent by the sigqueue().
pub const SI_QUEUE: i32 = 0x10002;
/// Signal generated by expiration of
/// a timer set by `timer_settime()`.
pub const SI_TIMER: i32 = 0x10003;
/// Signal generated by completion of
/// an asynchronous I/O request.
pub const SI_ASYNCIO: i32 = 0x10004;
/// Signal generated by arrival of a
/// message on an empty message queue.
pub const SI_MESGQ: i32 = 0x10005;
pub const SI_KERNEL: i32 = 0x10006;
/// Signal sent by `thr_kill`
pub const SI_LWP: i32 = 0x10007;
pub const SI_UNDEFINED: i32 = 0;

/// type of pointer to a signal function
///
/// Type of `typedef __sighandler_t *sig_t;`
pub type sig_t = usize;

/// Type of `typedef void __siginfohandler_t(int, struct __siginfo *, void *);`
pub type siginfohandler_t = usize;

pub type sigaltstack_t = stack_t;

/// take signal on alternate stack
pub const SS_ONSTACK: i32 = 0x0001;
/// disable taking signals on alternate stack
pub const SS_DISABLE: i32 = 0x0004;
/// minimum stack size
pub const MINSIGSTKSZ: usize = __MINSIGSTKSZ;
/// recommended stack size
pub const SIGSTKSZ: usize = MINSIGSTKSZ + 32768;

/// Structure used in sigaltstack call.  Its definition is always
/// needed for `__ucontext`.  If `__BSD_VISIBLE` is defined, the structure
/// tag is actually sigaltstack.
#[derive(Debug, Default, Clone)]
pub struct stack_t {
    /// signal stack base
    pub ss_sp: usize,
    /// signal stack length
    pub ss_size: size_t,
    /// SS_DISABLE and/or SS_ONSTACK
    pub ss_flags: i32,
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
#[derive(Debug, Default)]
pub struct sigstack_t {
    /// signal stack pointer
    pub ss_sp: usize,
    /// current status
    pub ss_onstack: i32,
}

/// Macro for converting signal number to a mask suitable for sigblock().
#[must_use]
pub const fn sigmask(m: i32) -> i32 {
    1 << (m - 1)
}

pub const BADSIG: sighandler_t = SIG_ERR;

/// Flags for sigprocmask:
/// block specified signal set
pub const SIG_BLOCK: i32 = 1;
/// unblock specified signal set
pub const SIG_UNBLOCK: i32 = 2;
/// set specified signal set
pub const SIG_SETMASK: i32 = 3;
