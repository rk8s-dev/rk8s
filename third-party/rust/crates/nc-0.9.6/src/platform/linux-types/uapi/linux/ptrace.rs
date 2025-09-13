// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/ptrace.h`
//!
//! structs and defines to help the user use the ptrace system call.
//! has the defines to get at the registers.

#![allow(clippy::module_name_repetitions)]

pub const PTRACE_TRACEME: i32 = 0;
pub const PTRACE_PEEKTEXT: i32 = 1;
pub const PTRACE_PEEKDATA: i32 = 2;
pub const PTRACE_PEEKUSR: i32 = 3;
pub const PTRACE_POKETEXT: i32 = 4;
pub const PTRACE_POKEDATA: i32 = 5;
pub const PTRACE_POKEUSR: i32 = 6;
pub const PTRACE_CONT: i32 = 7;
pub const PTRACE_KILL: i32 = 8;
pub const PTRACE_SINGLESTEP: i32 = 9;

pub const PTRACE_ATTACH: i32 = 16;
pub const PTRACE_DETACH: i32 = 17;

pub const PTRACE_SYSCALL: i32 = 24;

/// 0x4200-0x4300 are reserved for architecture-independent additions.
pub const PTRACE_SETOPTIONS: i32 = 0x4200;
pub const PTRACE_GETEVENTMSG: i32 = 0x4201;
pub const PTRACE_GETSIGINFO: i32 = 0x4202;
pub const PTRACE_SETSIGINFO: i32 = 0x4203;

/// Generic ptrace interface that exports the architecture specific regsets
/// using the corresponding `NT_*` types (which are also used in the core dump).
///
/// Please note that the `NT_PRSTATUS` note type in a core dump contains a full
/// `struct elf_prstatus`. But the `user_regset` for `NT_PRSTATUS` contains just the
/// `elf_gregset_t` that is the `pr_reg` field of `struct elf_prstatus`. For all the
/// other `user_regset` flavors, the `user_regset` layout and the ELF core dump note
/// payload are exactly the same layout.
///
/// This interface usage is as follows:
///  ```c
///  struct iovec iov = { buf, len};
///  ret = ptrace(PTRACE_GETREGSET/PTRACE_SETREGSET, pid, NT_XXX_TYPE, &iov);
///  ```
///
/// On the successful completion, iov.len will be updated by the kernel,
/// specifying how much the kernel has written/read to/from the user's iov.buf.
pub const PTRACE_GETREGSET: i32 = 0x4204;
pub const PTRACE_SETREGSET: i32 = 0x4205;

pub const PTRACE_SEIZE: i32 = 0x4206;
pub const PTRACE_INTERRUPT: i32 = 0x4207;
pub const PTRACE_LISTEN: i32 = 0x4208;

pub const PTRACE_PEEKSIGINFO: i32 = 0x4209;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ptrace_peeksiginfo_args_t {
    /// from which siginfo to start
    pub off: u64,

    pub flags: u32,

    /// how may siginfos to take
    pub nr: i32,
}

pub const PTRACE_GETSIGMASK: i32 = 0x420a;
pub const PTRACE_SETSIGMASK: i32 = 0x420b;

pub const PTRACE_SECCOMP_GET_FILTER: i32 = 0x420c;
pub const PTRACE_SECCOMP_GET_METADATA: i32 = 0x420d;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct seccomp_metadata_t {
    /// Input: which filter
    pub filter_off: u64,

    /// Output: filter's flags
    pub flags: u64,
}

pub const PTRACE_GET_SYSCALL_INFO: i32 = 0x420e;
pub const PTRACE_SYSCALL_INFO_NONE: i32 = 0;
pub const PTRACE_SYSCALL_INFO_ENTRY: i32 = 1;
pub const PTRACE_SYSCALL_INFO_EXIT: i32 = 2;
pub const PTRACE_SYSCALL_INFO_SECCOMP: i32 = 3;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ptrace_syscall_info_seccomp_entry_t {
    pub nr: u64,
    pub args: [u64; 6],
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ptrace_syscall_info_seccomp_exit_t {
    pub rval: i64,
    pub is_error: u8,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ptrace_syscall_info_seccomp_seccomp_t {
    pub nr: u64,
    pub args: [u64; 6],
    pub ret_data: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union ptrace_syscall_info_seccomp_t {
    pub entry: ptrace_syscall_info_seccomp_entry_t,

    pub exit: ptrace_syscall_info_seccomp_exit_t,

    pub seccomp: ptrace_syscall_info_seccomp_seccomp_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ptrace_syscall_info_t {
    /// `PTRACE_SYSCALL_INFO`_*
    pub op: u8,

    //__u32 arch __attribute__((__aligned__(sizeof(__u32))));
    pub arch: u32,

    pub instruction_pointer: u64,
    pub stack_pointer: u64,
    pub seccomp: ptrace_syscall_info_seccomp_t,
}

/// These values are stored in `task->ptrace_message`
/// by `tracehook_report_syscall_*` to describe the current syscall-stop.
pub const PTRACE_EVENTMSG_SYSCALL_ENTRY: i32 = 1;
pub const PTRACE_EVENTMSG_SYSCALL_EXIT: i32 = 2;

/// Read signals from a shared (process wide) queue
pub const PTRACE_PEEKSIGINFO_SHARED: i32 = 1;

/// Wait extended result codes for the above trace options.
pub const PTRACE_EVENT_FORK: i32 = 1;
pub const PTRACE_EVENT_VFORK: i32 = 2;
pub const PTRACE_EVENT_CLONE: i32 = 3;
pub const PTRACE_EVENT_EXEC: i32 = 4;
pub const PTRACE_EVENT_VFORK_DONE: i32 = 5;
pub const PTRACE_EVENT_EXIT: i32 = 6;
pub const PTRACE_EVENT_SECCOMP: i32 = 7;
/// Extended result codes which enabled by means other than options.
pub const PTRACE_EVENT_STOP: i32 = 128;

/// Options set using `PTRACE_SETOPTIONS` or using `PTRACE_SEIZE` @data param
pub const PTRACE_O_TRACESYSGOOD: i32 = 1;
pub const PTRACE_O_TRACEFORK: i32 = 1 << PTRACE_EVENT_FORK;
pub const PTRACE_O_TRACEVFORK: i32 = 1 << PTRACE_EVENT_VFORK;
pub const PTRACE_O_TRACECLONE: i32 = 1 << PTRACE_EVENT_CLONE;
pub const PTRACE_O_TRACEEXEC: i32 = 1 << PTRACE_EVENT_EXEC;
pub const PTRACE_O_TRACEVFORKDONE: i32 = 1 << PTRACE_EVENT_VFORK_DONE;
pub const PTRACE_O_TRACEEXIT: i32 = 1 << PTRACE_EVENT_EXIT;
pub const PTRACE_O_TRACESECCOMP: i32 = 1 << PTRACE_EVENT_SECCOMP;

/// eventless options
pub const PTRACE_O_EXITKILL: i32 = 1 << 20;
pub const PTRACE_O_SUSPEND_SECCOMP: i32 = 1 << 21;

pub const PTRACE_O_MASK: i32 = 0x0000_00ff | PTRACE_O_EXITKILL | PTRACE_O_SUSPEND_SECCOMP;
