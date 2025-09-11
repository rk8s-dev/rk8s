// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/x86/include/uapi/asm/ptrace-abi.h`

/// C ABI says these regs are callee-preserved. They aren't saved on kernel entry
/// unless syscall needs a complete, fully filled `struct pt_regs`.
pub const R15: i32 = 0;
pub const R14: i32 = 8;
pub const R13: i32 = 16;
pub const R12: i32 = 24;
pub const RBP: i32 = 32;
pub const RBX: i32 = 40;

/// These regs are callee-clobbered. Always saved on kernel entry.
pub const R11: i32 = 48;
pub const R10: i32 = 56;
pub const R9: i32 = 64;
pub const R8: i32 = 72;
pub const RAX: i32 = 80;
pub const RCX: i32 = 88;
pub const RDX: i32 = 96;
pub const RSI: i32 = 104;
pub const RDI: i32 = 112;
/// On syscall entry, this is syscall#. On CPU exception, this is error code.
/// On hw interrupt, it's IRQ number:
pub const ORIG_RAX: i32 = 120;
/// Return frame for iretq
pub const RIP: i32 = 128;
pub const CS: i32 = 136;
pub const EFLAGS: i32 = 144;
pub const RSP: i32 = 152;
pub const SS: i32 = 160;

/// top of stack page
pub const FRAME_SIZE: i32 = 168;

/// Arbitrarily choose the same ptrace numbers as used by the Sparc code.
pub const PTRACE_GETREGS: i32 = 12;
pub const PTRACE_SETREGS: i32 = 13;
pub const PTRACE_GETFPREGS: i32 = 14;
pub const PTRACE_SETFPREGS: i32 = 15;
pub const PTRACE_GETFPXREGS: i32 = 18;
pub const PTRACE_SETFPXREGS: i32 = 19;

pub const PTRACE_OLDSETOPTIONS: i32 = 21;

/// only useful for access 32bit programs / kernels
pub const PTRACE_GET_THREAD_AREA: i32 = 25;
pub const PTRACE_SET_THREAD_AREA: i32 = 26;

pub const PTRACE_ARCH_PRCTL: i32 = 30;

pub const PTRACE_SYSEMU: i32 = 31;
pub const PTRACE_SYSEMU_SINGLESTEP: i32 = 32;

/// resume execution until next branch
pub const PTRACE_SINGLEBLOCK: i32 = 33;
