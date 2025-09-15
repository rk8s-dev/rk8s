// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/prctl.h`

#![allow(clippy::module_name_repetitions)]

use core::ptr;

/// Values to pass as first argument to `prctl()`
///
/// Second arg is a signal
pub const PR_SET_PDEATHSIG: i32 = 1;
/// Second arg is a ptr to return the signal
pub const PR_GET_PDEATHSIG: i32 = 2;

/// Get/set `current->mm->dumpable`
pub const PR_GET_DUMPABLE: i32 = 3;
pub const PR_SET_DUMPABLE: i32 = 4;

/// Get/set unaligned access control bits (if meaningful)
pub const PR_GET_UNALIGN: i32 = 5;
pub const PR_SET_UNALIGN: i32 = 6;
/// silently fix up unaligned user accesses
pub const PR_UNALIGN_NOPRINT: i32 = 1;
/// generate SIGBUS on unaligned user access
pub const PR_UNALIGN_SIGBUS: i32 = 2;

/// Get/set whether or not to drop capabilities on `setuid()` away from
/// uid 0 (as per `security/commoncap.c`)
pub const PR_GET_KEEPCAPS: i32 = 7;
pub const PR_SET_KEEPCAPS: i32 = 8;

/// Get/set floating-point emulation control bits (if meaningful)
pub const PR_GET_FPEMU: i32 = 9;
pub const PR_SET_FPEMU: i32 = 10;
/// silently emulate fp operations accesses
pub const PR_FPEMU_NOPRINT: i32 = 1;
/// don't emulate fp operations, send `SIGFPE` instead
pub const PR_FPEMU_SIGFPE: i32 = 2;

/// Get/set floating-point exception mode (if meaningful)
pub const PR_GET_FPEXC: i32 = 11;
pub const PR_SET_FPEXC: i32 = 12;
/// Use FPEXC for FP exception enables
pub const PR_FP_EXC_SW_ENABLE: i32 = 0x80;
/// floating point divide by zero
pub const PR_FP_EXC_DIV: i32 = 0x0001_0000;
/// floating point overflow
pub const PR_FP_EXC_OVF: i32 = 0x0002_0000;
/// floating point underflow
pub const PR_FP_EXC_UND: i32 = 0x0004_0000;
/// floating point inexact result
pub const PR_FP_EXC_RES: i32 = 0x0008_0000;
/// floating point invalid operation
pub const PR_FP_EXC_INV: i32 = 0x0010_0000;
/// FP exceptions disabled
pub const PR_FP_EXC_DISABLED: i32 = 0;
/// async non-recoverable exc. mode
pub const PR_FP_EXC_NONRECOV: i32 = 1;
/// async recoverable exception mode
pub const PR_FP_EXC_ASYNC: i32 = 2;
/// precise exception mode
pub const PR_FP_EXC_PRECISE: i32 = 3;

/// Get/set whether we use statistical process timing or accurate timestamp
/// based process timing
pub const PR_GET_TIMING: i32 = 13;
pub const PR_SET_TIMING: i32 = 14;
/// Normal, traditional, statistical process timing
pub const PR_TIMING_STATISTICAL: i32 = 0;
/// Accurate timestamp based process timing
pub const PR_TIMING_TIMESTAMP: i32 = 1;

/// Set process name
pub const PR_SET_NAME: i32 = 15;
/// Get process name
pub const PR_GET_NAME: i32 = 16;

/// Get/set process endian
pub const PR_GET_ENDIAN: i32 = 19;
pub const PR_SET_ENDIAN: i32 = 20;
pub const PR_ENDIAN_BIG: i32 = 0;
/// True little endian mode
pub const PR_ENDIAN_LITTLE: i32 = 1;
/// `PowerPC` pseudo little endian
pub const PR_ENDIAN_PPC_LITTLE: i32 = 2;

/// Get/set process seccomp mode
pub const PR_GET_SECCOMP: i32 = 21;
pub const PR_SET_SECCOMP: i32 = 22;

/// Get/set the capability bounding set (as per `security/commoncap.c`)
pub const PR_CAPBSET_READ: i32 = 23;
pub const PR_CAPBSET_DROP: i32 = 24;

/// Get/set the process' ability to use the timestamp counter instruction
pub const PR_GET_TSC: i32 = 25;
pub const PR_SET_TSC: i32 = 26;
/// allow the use of the timestamp counter
pub const PR_TSC_ENABLE: i32 = 1;
/// throw a SIGSEGV instead of reading the TSC
pub const PR_TSC_SIGSEGV: i32 = 2;

/// Get/set securebits (as per `security/commoncap.c`)
pub const PR_GET_SECUREBITS: i32 = 27;
pub const PR_SET_SECUREBITS: i32 = 28;

/// Get/set the timerslack as used by `poll/select/nanosleep`
/// A value of 0 means "use default"
pub const PR_SET_TIMERSLACK: i32 = 29;
pub const PR_GET_TIMERSLACK: i32 = 30;

pub const PR_TASK_PERF_EVENTS_DISABLE: i32 = 31;
pub const PR_TASK_PERF_EVENTS_ENABLE: i32 = 32;

/// Set early/late kill mode for hwpoison memory corruption.
/// This influences when the process gets killed on a memory corruption.
pub const PR_MCE_KILL: i32 = 33;
pub const PR_MCE_KILL_CLEAR: i32 = 0;
pub const PR_MCE_KILL_SET: i32 = 1;

pub const PR_MCE_KILL_LATE: i32 = 0;
pub const PR_MCE_KILL_EARLY: i32 = 1;
pub const PR_MCE_KILL_DEFAULT: i32 = 2;

pub const PR_MCE_KILL_GET: i32 = 34;

/// Tune up process memory map specifics.
pub const PR_SET_MM: i32 = 35;
pub const PR_SET_MM_START_CODE: i32 = 1;
pub const PR_SET_MM_END_CODE: i32 = 2;
pub const PR_SET_MM_START_DATA: i32 = 3;
pub const PR_SET_MM_END_DATA: i32 = 4;
pub const PR_SET_MM_START_STACK: i32 = 5;
pub const PR_SET_MM_START_BRK: i32 = 6;
pub const PR_SET_MM_BRK: i32 = 7;
pub const PR_SET_MM_ARG_START: i32 = 8;
pub const PR_SET_MM_ARG_END: i32 = 9;
pub const PR_SET_MM_ENV_START: i32 = 10;
pub const PR_SET_MM_ENV_END: i32 = 11;
pub const PR_SET_MM_AUXV: i32 = 12;
pub const PR_SET_MM_EXE_FILE: i32 = 13;
pub const PR_SET_MM_MAP: i32 = 14;
pub const PR_SET_MM_MAP_SIZE: i32 = 15;

/// This structure provides new memory descriptor
/// map which mostly modifies `/proc/pid/stat[m]`
/// output for a task. This mostly done in a
/// sake of checkpoint/restore functionality.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct prctl_mm_map_t {
    /// code section bounds
    pub start_code: u64,
    pub end_code: u64,
    /// data section bounds
    pub start_data: u64,
    pub end_data: u64,
    /// heap for `brk()` syscall
    pub start_brk: u64,
    pub brk: u64,
    /// stack starts at
    pub start_stack: u64,
    /// command line arguments bounds
    pub arg_start: u64,
    pub arg_end: u64,
    /// environment variables bounds
    pub env_start: u64,
    pub env_end: u64,
    /// auxiliary vector
    pub auxv: *mut u64,
    /// vector size
    pub auxv_size: u32,
    /// /proc/$pid/exe link file
    pub exe_fd: u32,
}

impl Default for prctl_mm_map_t {
    fn default() -> Self {
        Self {
            start_code: 0,
            end_code: 0,
            start_data: 0,
            end_data: 0,
            start_brk: 0,
            brk: 0,
            start_stack: 0,
            arg_start: 0,
            arg_end: 0,
            env_start: 0,
            env_end: 0,
            auxv: ptr::null_mut::<u64>(),
            auxv_size: 0,
            exe_fd: 0,
        }
    }
}

/// Set specific pid that is allowed to ptrace the current task.
/// A value of 0 mean "no process".
pub const PR_SET_PTRACER: i32 = 0x5961_6d61;
pub const PR_SET_PTRACER_ANY: u64 = u64::MAX; // ((unsigned long)-1);

pub const PR_SET_CHILD_SUBREAPER: i32 = 36;
pub const PR_GET_CHILD_SUBREAPER: i32 = 37;

/// If `no_new_privs` is set, then operations that grant new privileges (i.e.
/// execve) will either fail or not grant them.  This affects suid/sgid,
/// file capabilities, and LSMs.
///
/// Operations that merely manipulate or drop existing privileges (setresuid,
/// capset, etc.) will still work.  Drop those privileges if you want them gone.
/// Changing LSM security domain is considered a new privilege.  So, for example,
/// asking selinux for a specific new context (e.g. with runcon) will result
/// in execve returning `-EPERM`.
/// See `Documentation/userspace-api/no_new_privs.rst` for more details.
pub const PR_SET_NO_NEW_PRIVS: i32 = 38;
pub const PR_GET_NO_NEW_PRIVS: i32 = 39;

pub const PR_GET_TID_ADDRESS: i32 = 40;

pub const PR_SET_THP_DISABLE: i32 = 41;
pub const PR_GET_THP_DISABLE: i32 = 42;

/// No longer implemented, but left here to ensure the numbers stay reserved:
pub const PR_MPX_ENABLE_MANAGEMENT: i32 = 43;
pub const PR_MPX_DISABLE_MANAGEMENT: i32 = 44;

pub const PR_SET_FP_MODE: i32 = 45;
pub const PR_GET_FP_MODE: i32 = 46;
/// 64b FP registers
pub const PR_FP_MODE_FR: i32 = 1 << 0;
/// 32b compatibility
pub const PR_FP_MODE_FRE: i32 = 1 << 1;

/// Control the ambient capability set
pub const PR_CAP_AMBIENT: i32 = 47;
pub const PR_CAP_AMBIENT_IS_SET: i32 = 1;
pub const PR_CAP_AMBIENT_RAISE: i32 = 2;
pub const PR_CAP_AMBIENT_LOWER: i32 = 3;
pub const PR_CAP_AMBIENT_CLEAR_ALL: i32 = 4;

/// arm64 Scalable Vector Extension controls
/// Flag values must be kept in sync with ptrace `NT_ARM_SVE` interface
/// set task vector length
pub const PR_SVE_SET_VL: i32 = 50;
/// defer effect until exec
pub const PR_SVE_SET_VL_ONEXEC: i32 = 1 << 18;
/// get task vector length
pub const PR_SVE_GET_VL: i32 = 51;
/// Bits common to `PR_SVE_SET_VL` and `PR_SVE_GET_VL`
pub const PR_SVE_VL_LEN_MASK: i32 = 0xffff;
/// inherit across exec
pub const PR_SVE_VL_INHERIT: i32 = 1 << 17;

/// Per task speculation control
pub const PR_GET_SPECULATION_CTRL: i32 = 52;
pub const PR_SET_SPECULATION_CTRL: i32 = 53;
/// Speculation control variants
pub const PR_SPEC_STORE_BYPASS: i32 = 0;
pub const PR_SPEC_INDIRECT_BRANCH: i32 = 1;
/// Return and control values for `PR_SET/GET_SPECULATION_CTRL`
pub const PR_SPEC_NOT_AFFECTED: i32 = 0;
pub const PR_SPEC_PRCTL: u64 = 1 << 0;
pub const PR_SPEC_ENABLE: u64 = 1 << 1;
pub const PR_SPEC_DISABLE: u64 = 1 << 2;
pub const PR_SPEC_FORCE_DISABLE: u64 = 1 << 3;
pub const PR_SPEC_DISABLE_NOEXEC: u64 = 1 << 4;

/// Reset arm64 pointer authentication keys
pub const PR_PAC_RESET_KEYS: i32 = 54;
pub const PR_PAC_APIAKEY: u64 = 1 << 0;
pub const PR_PAC_APIBKEY: u64 = 1 << 1;
pub const PR_PAC_APDAKEY: u64 = 1 << 2;
pub const PR_PAC_APDBKEY: u64 = 1 << 3;
pub const PR_PAC_APGAKEY: u64 = 1 << 4;

/// Tagged user address controls for arm64
pub const PR_SET_TAGGED_ADDR_CTRL: i32 = 55;
pub const PR_GET_TAGGED_ADDR_CTRL: i32 = 56;
pub const PR_TAGGED_ADDR_ENABLE: u64 = 1 << 0;

/// Control reclaim behavior when allocating memory
pub const PR_SET_IO_FLUSHER: i32 = 57;
pub const PR_GET_IO_FLUSHER: i32 = 58;
