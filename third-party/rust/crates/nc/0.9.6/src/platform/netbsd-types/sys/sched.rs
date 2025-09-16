// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/sched.h`

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct sched_param_t {
    pub sched_priority: i32,
}

/// Scheduling policies required by IEEE Std 1003.1-2001
pub const SCHED_NONE: i32 = -1;
pub const SCHED_OTHER: i32 = 0;
pub const SCHED_FIFO: i32 = 1;
pub const SCHED_RR: i32 = 2;

/// CPU states.
/// XXX Not really scheduler state, but no other good place to put
/// it right now, and it really is per-CPU.
pub const CP_USER: i32 = 0;
pub const CP_NICE: i32 = 1;
pub const CP_SYS: i32 = 2;
pub const CP_INTR: i32 = 3;
pub const CP_IDLE: i32 = 4;
pub const CPUSTATES: i32 = 5;

/// Flags passed to the Linux-compatible __clone(2) system call.
///
/// signal to be sent at exit
pub const CLONE_CSIGNAL: i32 = 0x0000_00ff;
/// share address space
pub const CLONE_VM: i32 = 0x0000_0100;
/// share "file system" info
pub const CLONE_FS: i32 = 0x0000_0200;
/// share file descriptors
pub const CLONE_FILES: i32 = 0x0000_0400;
/// share signal actions
pub const CLONE_SIGHAND: i32 = 0x0000_0800;
/// share process ID
pub const CLONE_PID: i32 = 0x0000_1000;
/// ptrace(2) continues on child
pub const CLONE_PTRACE: i32 = 0x0000_2000;
/// parent blocks until child exits
pub const CLONE_VFORK: i32 = 0x0000_4000;
