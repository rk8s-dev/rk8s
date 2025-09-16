// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/signal-defs.h`

/// `SA_FLAGS` values:
///
/// - `SA_NOCLDSTOP` flag to turn off SIGCHLD when children stop.
/// - `SA_NOCLDWAIT` flag on SIGCHLD to inhibit zombies.
/// - `SA_SIGINFO` delivers the signal with SIGINFO structs.
/// - `SA_ONSTACK` indicates that a registered `stack_t` will be used.
/// - `SA_RESTART` flag to get restarting signals (which were the default long ago)
/// - `SA_NODEFER` prevents the current signal from being masked in the handler.
/// - `SA_RESETHAND` clears the handler when the signal is delivered.
/// - `SA_UNSUPPORTED` is a flag bit that will never be supported. Kernels from
///   before the introduction of `SA_UNSUPPORTED` did not clear unknown bits from
///   `sa_flags` when read using the oldact argument to sigaction and `rt_sigaction`,
///   so this bit allows flag bit support to be detected from userspace while
///   allowing an old kernel to be distinguished from a kernel that supports every
///   flag bit.
/// - `SA_EXPOSE_TAGBITS` exposes an architecture-defined set of tag bits in
///   `siginfo.si_addr`.
///
/// - `SA_ONESHOT` and `SA_NOMASK` are the historical Linux names for the Single
///   Unix names RESETHAND and NODEFER respectively.
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SA_NOCLDSTOP: usize = 0x0000_0001;
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SA_NOCLDWAIT: usize = 0x0000_0002;
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SA_SIGINFO: usize = 0x0000_0004;
/* 0x00000008 used on alpha, mips, parisc */
/* 0x00000010 used on alpha, parisc */
/* 0x00000020 used on alpha, parisc, sparc */
/* 0x00000040 used on alpha, parisc */
/* 0x00000080 used on parisc */
/* 0x00000100 used on sparc */
/* 0x00000200 used on sparc */
pub const SA_UNSUPPORTED: usize = 0x0000_0400;
pub const SA_EXPOSE_TAGBITS: usize = 0x0000_0800;
/* 0x00010000 used on mips */
/* 0x01000000 used on x86 */
/* 0x02000000 used on x86 */
// New architectures should not define the obsolete
// `SA_RESTORER` 0x04000000
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SA_ONSTACK: usize = 0x0800_0000;
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SA_RESTART: usize = 0x1000_0000;
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SA_NODEFER: usize = 0x4000_0000;
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SA_RESETHAND: usize = 0x8000_0000;

#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SA_NOMASK: usize = SA_NODEFER;
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SA_ONESHOT: usize = SA_RESETHAND;

/// for blocking signals
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SIG_BLOCK: i32 = 0;
/// for unblocking signals
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SIG_UNBLOCK: i32 = 1;
/// for setting the signal mask
#[cfg(not(any(target_arch = "mips", target_arch = "mips64")))]
pub const SIG_SETMASK: i32 = 2;

pub type signalfn_t = fn(i32);

/// Type of a signal handler.
/// `signalfn_t` as usize
pub type sighandler_t = usize;

/// The restore function should be written with assembly, or naked rust function,
/// which does not modify stack frame.
pub type restorefn_t = unsafe extern "C" fn();

/// `restorefn_t` as usize
pub type sigrestore_t = Option<restorefn_t>;

/// default signal handling
pub const SIG_DFL: sighandler_t = 0;
/// ignore signal
pub const SIG_IGN: sighandler_t = 1;
/// error return from signal
pub const SIG_ERR: sighandler_t = !0 as sighandler_t;
