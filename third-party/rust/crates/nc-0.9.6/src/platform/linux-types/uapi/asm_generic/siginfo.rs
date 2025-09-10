// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/siginfo.h`

#![allow(clippy::module_name_repetitions)]
#![allow(clippy::uninlined_format_args)]

use core::fmt;
use core::mem::size_of;

use crate::{clock_t, pid_t, timer_t, uid_t};

#[repr(C)]
#[derive(Clone, Copy)]
pub union sigval_t {
    pub sival_int: i32,
    pub sival_ptr: usize,
}

impl fmt::Debug for sigval_t {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let val = unsafe { self.sival_ptr };
        write!(f, "sigval_t: {}", val)
    }
}

impl Default for sigval_t {
    fn default() -> Self {
        Self { sival_ptr: 0 }
    }
}

pub const SI_MAX_SIZE: usize = 128;

/// The default `si_band` type is "long", as specified by POSIX.
/// However, some architectures want to override this to "int"
/// for historical compatibility reasons, so we allow that.
pub type arch_si_band_t = isize;

pub type arch_si_clock_t = clock_t;

/// `kill()`
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct si_kill_t {
    /// sender's pid
    pub pid: pid_t,
    /// sender's uid
    pub uid: uid_t,
}

/// POSIX.1b timers
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct si_timer_t {
    /// timer id
    pub tid: timer_t,
    /// overrun count
    pub overrun: i32,
    /// same as below
    pub sigval: sigval_t,
    /// not to be passed to user
    sys_private: i32,
}

/// POSIX.1b signals
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct si_rt_t {
    /// sender's pid
    pub pid: pid_t,
    /// sender's uid
    pub uid: uid_t,
    pub sigval: sigval_t,
}

/// SIGCHLD
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct si_sigchld_t {
    /// which child
    pub pid: pid_t,
    /// sender's uid
    pub uid: uid_t,
    /// exit code
    pub status: i32,
    pub utime: arch_si_clock_t,
    pub stime: arch_si_clock_t,
}

/// SIGPOLL
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct si_sigpoll_t {
    /// `POLL_IN`, `POLL_OUT`, `POLL_MSG`
    pub band: arch_si_band_t,
    pub fd: i32,
}

/// SIGSYS
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct si_sigsys_t {
    /// calling user insn
    pub call_addr: usize,
    /// triggering system call number
    pub syscall: i32,
    /// `AUDIT_ARCH`_* of syscall
    pub arch: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union sifields_t {
    pub kill: si_kill_t,

    pub timer: si_timer_t,

    pub rt: si_rt_t,

    pub sigchld: si_sigchld_t,
    pub sigpoll: si_sigpoll_t,

    pub sigsys: si_sigsys_t,
}

impl Default for sifields_t {
    fn default() -> Self {
        Self {
            rt: si_rt_t::default(),
        }
    }
}

// TODO(Shaohua): Move to arch specific types.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct siginfo_intern_t {
    pub si_signo: i32,
    pub si_errno: i32,
    pub si_code: i32,
    pub sifields: sifields_t,
}
//struct __siginfo_t {				\
//	int si_signo;			\
//	int si_code;			\
//	int si_errno;			\
//	union __sifields _sifields;	\
//}

#[repr(C)]
#[derive(Clone, Copy)]
pub union siginfo_t {
    pub siginfo: siginfo_intern_t,
    si_pad: [u8; SI_MAX_SIZE / size_of::<i32>()],
}

impl Default for siginfo_t {
    fn default() -> Self {
        Self {
            si_pad: [0; SI_MAX_SIZE / size_of::<i32>()],
        }
    }
}

// How these fields are to be accessed.
//#define si_pid		_sifields._kill._pid
//#define si_uid		_sifields._kill._uid
//#define si_tid		_sifields._timer._tid
//#define si_overrun	_sifields._timer._overrun
//#define si_sys_private  _sifields._timer._sys_private
//#define si_status	_sifields._sigchld._status
//#define si_utime	_sifields._sigchld._utime
//#define si_stime	_sifields._sigchld._stime
//#define si_value	_sifields._rt._sigval
//#define si_int		_sifields._rt._sigval.sival_int
//#define si_ptr		_sifields._rt._sigval.sival_ptr
//#define si_addr		_sifields._sigfault._addr
//#ifdef __ARCH_SI_TRAPNO
//#define si_trapno	_sifields._sigfault._trapno
//#endif
//#define si_addr_lsb	_sifields._sigfault._addr_lsb
//#define si_lower	_sifields._sigfault._addr_bnd._lower
//#define si_upper	_sifields._sigfault._addr_bnd._upper
//#define si_pkey		_sifields._sigfault._addr_pkey._pkey
//#define si_band		_sifields._sigpoll._band
//#define si_fd		_sifields._sigpoll._fd
//#define si_call_addr	_sifields._sigsys._call_addr
//#define si_syscall	_sifields._sigsys._syscall
//#define si_arch		_sifields._sigsys._arch

/// `si_code` values
/// Digital reserves positive values for kernel-generated signals.
/// sent by kill, sigsend, raise
pub const SI_USER: i32 = 0;
/// sent by the kernel from somewhere
pub const SI_KERNEL: i32 = 0x80;
/// sent by sigqueue
pub const SI_QUEUE: i32 = -1;
/// sent by timer expiration
pub const SI_TIMER: i32 = -2;
/// sent by real time mesq state change
pub const SI_MESGQ: i32 = -3;
/// sent by AIO completion
pub const SI_ASYNCIO: i32 = -4;
/// sent by queued SIGIO
pub const SI_SIGIO: i32 = -5;
/// sent by tkill system call
pub const SI_TKILL: i32 = -6;
/// sent by `execve()` killing subsidiary threads
pub const SI_DETHREAD: i32 = -7;
/// sent by glibc async name lookup completion
pub const SI_ASYNCNL: i32 = -60;

//#define SI_FROMUSER(siptr)	((siptr)->si_code <= 0)
//#define SI_FROMKERNEL(siptr)	((siptr)->si_code > 0)

/// SIGILL `si_codes`
/// illegal opcode
pub const ILL_ILLOPC: i32 = 1;
/// illegal operand
pub const ILL_ILLOPN: i32 = 2;
/// illegal addressing mode
pub const ILL_ILLADR: i32 = 3;
/// illegal trap
pub const ILL_ILLTRP: i32 = 4;
/// privileged opcode
pub const ILL_PRVOPC: i32 = 5;
/// privileged register
pub const ILL_PRVREG: i32 = 6;
/// coprocessor error
pub const ILL_COPROC: i32 = 7;
/// internal stack error
pub const ILL_BADSTK: i32 = 8;
/// unimplemented instruction address
pub const ILL_BADIADDR: i32 = 9;
/// illegal break
pub const __ILL_BREAK: i32 = 10;
/// bundle-update (modification) in progress
pub const __ILL_BNDMOD: i32 = 11;
pub const NSIGILL: i32 = 11;

/// SIGFPE `si_codes`
/// integer divide by zero
pub const FPE_INTDIV: i32 = 1;
/// integer overflow
pub const FPE_INTOVF: i32 = 2;
/// floating point divide by zero
pub const FPE_FLTDIV: i32 = 3;
/// floating point overflow
pub const FPE_FLTOVF: i32 = 4;
/// floating point underflow
pub const FPE_FLTUND: i32 = 5;
/// floating point inexact result
pub const FPE_FLTRES: i32 = 6;
/// floating point invalid operation
pub const FPE_FLTINV: i32 = 7;
/// subscript out of range
pub const FPE_FLTSUB: i32 = 8;
/// decimal overflow
pub const __FPE_DECOVF: i32 = 9;
/// decimal division by zero
pub const __FPE_DECDIV: i32 = 10;
/// packed decimal error
pub const __FPE_DECERR: i32 = 11;
/// invalid ASCII digit
pub const __FPE_INVASC: i32 = 12;
/// invalid decimal digit
pub const __FPE_INVDEC: i32 = 13;
/// undiagnosed floating-point exception
pub const FPE_FLTUNK: i32 = 14;
/// trap on condition
pub const FPE_CONDTRAP: i32 = 15;
pub const NSIGFPE: i32 = 15;

/// SIGSEGV `si_codes`
/// address not mapped to object
pub const SEGV_MAPERR: i32 = 1;
/// invalid permissions for mapped object
pub const SEGV_ACCERR: i32 = 2;
/// failed address bound checks
pub const SEGV_BNDERR: i32 = 3;
//#ifdef __ia64__
//# define __SEGV_PSTKOVF	4	/* paragraph stack overflow */
/// failed protection key checks
pub const SEGV_PKUERR: i32 = 4;
/// ADI not enabled for mapped object
pub const SEGV_ACCADI: i32 = 5;
/// Disrupting MCD error
pub const SEGV_ADIDERR: i32 = 6;
/// Precise MCD exception
pub const SEGV_ADIPERR: i32 = 7;
pub const NSIGSEGV: i32 = 7;

/// SIGBUS `si_codes`
/// invalid address alignment
pub const BUS_ADRALN: i32 = 1;
/// non-existent physical address
pub const BUS_ADRERR: i32 = 2;
/// object specific hardware error
pub const BUS_OBJERR: i32 = 3;
/// hardware memory error consumed on a machine check: action required
pub const BUS_MCEERR_AR: i32 = 4;
/// hardware memory error detected in process but not consumed: action optional
pub const BUS_MCEERR_AO: i32 = 5;
pub const NSIGBUS: i32 = 5;

/// SIGTRAP `si_codes`
/// process breakpoint
pub const TRAP_BRKPT: i32 = 1;
/// process trace trap
pub const TRAP_TRACE: i32 = 2;
/// process taken branch trap
pub const TRAP_BRANCH: i32 = 3;
/// hardware breakpoint/watchpoint
pub const TRAP_HWBKPT: i32 = 4;
/// undiagnosed trap
pub const TRAP_UNK: i32 = 5;
pub const NSIGTRAP: i32 = 5;

/// There is an additional set of SIGTRAP `si_codes` used by ptrace
/// that are of the form: `((PTRACE_EVENT_XXX << 8) | SIGTRAP)`
///
/// SIGCHLD `si_codes`
/// child has exited
pub const CLD_EXITED: i32 = 1;
/// child was killed
pub const CLD_KILLED: i32 = 2;
/// child terminated abnormally
pub const CLD_DUMPED: i32 = 3;
/// traced child has trapped
pub const CLD_TRAPPED: i32 = 4;
/// child has stopped
pub const CLD_STOPPED: i32 = 5;
/// stopped child has continued
pub const CLD_CONTINUED: i32 = 6;
pub const NSIGCHLD: i32 = 6;

/// SIGPOLL (or any other signal without signal specific `si_codes`) `si_codes`
/// data input available
pub const POLL_IN: i32 = 1;
/// output buffers available
pub const POLL_OUT: i32 = 2;
/// input message available
pub const POLL_MSG: i32 = 3;
/// i/o error
pub const POLL_ERR: i32 = 4;
/// high priority input available
pub const POLL_PRI: i32 = 5;
/// device disconnected
pub const POLL_HUP: i32 = 6;
pub const NSIGPOLL: i32 = 6;

/// SIGSYS `si_codes`
/// seccomp triggered
/// Return from `SYS_SECCOMP` as it is already used by an syscall num.
pub const SYS_SECCOMP_: i32 = 1;
pub const NSIGSYS: i32 = 1;

/// SIGEMT `si_codes`
/// tag overflow
pub const EMT_TAGOVF: i32 = 1;
pub const NSIGEMT: i32 = 1;

/// sigevent definitions
///
/// It seems likely that `SIGEV_THREAD` will have to be handled from
/// userspace, libpthread transmuting it to `SIGEV_SIGNAL`, which the
/// thread manager then catches and does the appropriate nonsense.
/// However, everything is written out here so as to not get lost.
///
/// notify via signal
pub const SIGEV_SIGNAL: i32 = 0;
/// other notification: meaningless
pub const SIGEV_NONE: i32 = 1;
/// deliver via thread creation
pub const SIGEV_THREAD: i32 = 2;
/// deliver to thread
pub const SIGEV_THREAD_ID: i32 = 4;

/// This works because the alignment is ok on all current architectures
/// but we leave open this being overridden in the future
const ARCH_SIGEV_PREAMBLE_SIZE: usize = size_of::<i32>() * 2 + size_of::<sigval_t>();

pub const SIGEV_MAX_SIZE: usize = 64;
pub const SIGEV_PAD_SIZE: usize = (SIGEV_MAX_SIZE - ARCH_SIGEV_PREAMBLE_SIZE) / size_of::<i32>();

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct sigev_thread_t {
    pub function: usize,
    /// really `pthread_attr_t`
    pub attribute: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union sigev_un_t {
    pad: [i32; SIGEV_PAD_SIZE],
    pub tid: i32,
    pub sigev_thread: sigev_thread_t,
}

impl Default for sigev_un_t {
    fn default() -> Self {
        Self {
            pad: [0; SIGEV_PAD_SIZE],
        }
    }
}

impl fmt::Debug for sigev_un_t {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let tid = unsafe { self.tid };
        write!(f, "sigev_un_t.tid = {}", tid)
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct sigevent_t {
    pub sigev_value: sigval_t,
    pub sigev_signo: i32,
    pub sigev_notify: i32,
    pub sigev_un: sigev_un_t,
}

//#define sigev_notify_function	_sigev_un._sigev_thread._function
//#define sigev_notify_attributes	_sigev_un._sigev_thread._attribute
//#define sigev_notify_thread_id	 _sigev_un._tid
