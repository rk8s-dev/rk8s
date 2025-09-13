// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/seccomp.h`

#![allow(clippy::module_name_repetitions)]

use crate::{IO, IOR, IOW, IOWR};

/// Valid values for seccomp.mode and prctl(`PR_SET_SECCOMP`, `<mode>`)
/// seccomp is not in use.
pub const SECCOMP_MODE_DISABLED: u32 = 0;
/// uses hard-coded filter.
pub const SECCOMP_MODE_STRICT: u32 = 1;
/// uses user-supplied filter.
pub const SECCOMP_MODE_FILTER: u32 = 2;

/// Valid operations for seccomp syscall.
pub const SECCOMP_SET_MODE_STRICT: u32 = 0;
pub const SECCOMP_SET_MODE_FILTER: u32 = 1;
pub const SECCOMP_GET_ACTION_AVAIL: u32 = 2;
pub const SECCOMP_GET_NOTIF_SIZES: u32 = 3;

/// Valid flags for `SECCOMP_SET_MODE_FILTER`
pub const SECCOMP_FILTER_FLAG_TSYNC: usize = 1;
pub const SECCOMP_FILTER_FLAG_LOG: usize = 1 << 1;
pub const SECCOMP_FILTER_FLAG_SPEC_ALLOW: usize = 1 << 2;
pub const SECCOMP_FILTER_FLAG_NEW_LISTENER: usize = 1 << 3;

/// All BPF programs must return a 32-bit value.
///
/// The bottom 16-bits are for optional return data.
/// The upper 16-bits are ordered from least permissive values to most,
/// as a signed value (so 0x8000000 is negative).
///
/// The ordering ensures that a `min_t()` over composed return values always
/// selects the least permissive choice.
/// kill the process
pub const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
/// kill the thread
pub const SECCOMP_RET_KILL_THREAD: u32 = 0x0000_0000;
pub const SECCOMP_RET_KILL: u32 = SECCOMP_RET_KILL_THREAD;
/// disallow and force a SIGSYS
pub const SECCOMP_RET_TRAP: u32 = 0x0003_0000;
/// returns an errno
pub const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
/// notifies userspace
pub const SECCOMP_RET_USER_NOTIF: u32 = 0x7fc0_0000;
/// pass to a tracer or disallow
pub const SECCOMP_RET_TRACE: u32 = 0x7ff0_0000;
/// allow after logging
pub const SECCOMP_RET_LOG: u32 = 0x7ffc_0000;
/// allow
pub const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;

/// Masks for the return value sections.
pub const SECCOMP_RET_ACTION_FULL: u32 = 0xffff_0000;
pub const SECCOMP_RET_ACTION: u32 = 0x7fff_0000;
pub const SECCOMP_RET_DATA: u32 = 0x0000_ffff;

/// struct `seccomp_data` - the format the BPF program executes over.
///
/// @nr: the system call number
/// @arch: indicates system call convention as an `AUDIT_ARCH_*` value
///        as defined in `<linux/audit.h>`.
/// `@instruction_pointer`: at the time of the system call.
/// @args: up to 6 system call arguments always stored as 64-bit values
///        regardless of the architecture.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct seccomp_data_t {
    pub nr: i32,
    pub arch: u32,
    pub instruction_pointer: u64,
    pub args: [u64; 6],
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct seccomp_notif_sizes_t {
    pub seccomp_notif: u16,
    pub seccomp_notif_resp: u16,
    pub seccomp_data: u16,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct seccomp_notif_t {
    pub id: u64,
    pub pid: u32,
    pub flags: u32,
    pub data: seccomp_data_t,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct seccomp_notif_resp_t {
    pub id: u64,
    pub val: i64,
    pub error: i32,
    pub flags: u32,
}

pub const SECCOMP_IOC_MAGIC: u8 = b'!';

#[must_use]
#[inline]
pub const fn SECCOMP_IO(nr: u32) -> u32 {
    IO(SECCOMP_IOC_MAGIC, nr)
}

#[must_use]
#[inline]
pub const fn SECCOMP_IOR<T>(nr: u32) -> u32 {
    IOR::<T>(SECCOMP_IOC_MAGIC, nr)
}

#[must_use]
#[inline]
pub const fn SECCOMP_IOW<T>(nr: u32) -> u32 {
    IOW::<T>(SECCOMP_IOC_MAGIC, nr)
}

#[must_use]
#[inline]
pub const fn SECCOMP_IOWR<T>(nr: u32) -> u32 {
    IOWR::<T>(SECCOMP_IOC_MAGIC, nr)
}

/// Flags for seccomp notification fd ioctl.
pub const SECCOMP_IOCTL_NOTIF_RECV: u32 = SECCOMP_IOWR::<seccomp_notif_t>(0);

pub const SECCOMP_IOCTL_NOTIF_SEND: u32 = SECCOMP_IOWR::<seccomp_notif_resp_t>(1);

pub const SECCOMP_IOCTL_NOTIF_ID_VALID: u32 = SECCOMP_IOR::<u64>(2);
