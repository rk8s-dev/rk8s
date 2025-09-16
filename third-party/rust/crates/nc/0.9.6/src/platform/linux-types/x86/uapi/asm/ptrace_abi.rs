// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/x86/include/uapi/asm/ptrace-abi.h`

pub const EBX: i32 = 0;
pub const ECX: i32 = 1;
pub const EDX: i32 = 2;
pub const ESI: i32 = 3;
pub const EDI: i32 = 4;
pub const EBP: i32 = 5;
pub const EAX: i32 = 6;
pub const DS: i32 = 7;
pub const ES: i32 = 8;
pub const FS: i32 = 9;
pub const GS: i32 = 10;
pub const ORIG_EAX: i32 = 11;
pub const EIP: i32 = 12;
pub const CS: i32 = 13;
pub const EFL: i32 = 14;
pub const UESP: i32 = 15;
pub const SS: i32 = 16;
pub const FRAME_SIZE: i32 = 17;

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

pub const PTRACE_SYSEMU: i32 = 31;
pub const PTRACE_SYSEMU_SINGLESTEP: i32 = 32;

/// resume execution until next branch
pub const PTRACE_SINGLEBLOCK: i32 = 33;
