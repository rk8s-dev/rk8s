// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/sched.h`

mod cpu;
mod sched_types;
pub use cpu::*;
pub use sched_types::*;

use crate::uintptr_t;

/// cloning flags:
/// signal mask to be sent at exit
pub const CSIGNAL: i32 = 0x0000_00ff;
/// set if VM shared between processes
pub const CLONE_VM: i32 = 0x0000_0100;
/// set if fs info shared between processes
pub const CLONE_FS: i32 = 0x0000_0200;
/// set if open files shared between processes
pub const CLONE_FILES: i32 = 0x0000_0400;
/// set if signal handlers and blocked signals shared
pub const CLONE_SIGHAND: i32 = 0x0000_0800;
/// set if a pidfd should be placed in parent
pub const CLONE_PIDFD: i32 = 0x0000_1000;
/// set if we want to let tracing continue on the child too
pub const CLONE_PTRACE: i32 = 0x0000_2000;
/// set if the parent wants the child to wake it up on `mm_release`
pub const CLONE_VFORK: i32 = 0x0000_4000;
/// set if we want to have the same parent as the cloner
pub const CLONE_PARENT: i32 = 0x0000_8000;
/// Same thread group?
pub const CLONE_THREAD: i32 = 0x0001_0000;
/// New mount namespace group
pub const CLONE_NEWNS: i32 = 0x0002_0000;
/// share system V `SEM_UNDO` semantics
pub const CLONE_SYSVSEM: i32 = 0x0004_0000;
/// create a new TLS for the child
pub const CLONE_SETTLS: i32 = 0x0008_0000;
/// set the TID in the parent
pub const CLONE_PARENT_SETTID: i32 = 0x0010_0000;
/// clear the TID in the child
pub const CLONE_CHILD_CLEARTID: i32 = 0x0020_0000;
/// Unused, ignored
pub const CLONE_DETACHED: i32 = 0x0040_0000;
/// set if the tracing process can't force `CLONE_PTRACE` on this clone
pub const CLONE_UNTRACED: i32 = 0x0080_0000;
/// set the TID in the child
pub const CLONE_CHILD_SETTID: i32 = 0x0100_0000;
/// New cgroup namespace
pub const CLONE_NEWCGROUP: i32 = 0x0200_0000;
/// New utsname namespace
pub const CLONE_NEWUTS: i32 = 0x0400_0000;
/// New ipc namespace
pub const CLONE_NEWIPC: i32 = 0x0800_0000;
/// New user namespace
pub const CLONE_NEWUSER: i32 = 0x1000_0000;
/// New pid namespace
pub const CLONE_NEWPID: i32 = 0x2000_0000;
/// New network namespace
pub const CLONE_NEWNET: i32 = 0x4000_0000;
/// Clone io context
#[allow(overflowing_literals)]
pub const CLONE_IO: i32 = 0x8000_0000;

/// Flags for the `clone3()` syscall.
/// Clear any signal handler and reset to `SIG_DFL`.
pub const CLONE_CLEAR_SIGHAND: u64 = 0x0001_0000_0000;
/// Clone into a specific cgroup given the right permissions.
pub const CLONE_INTO_CGROUP: u64 = 0x0002_0000_0000;

/// cloning flags intersect with CSIGNAL so can be used with unshare and
/// `clone3()` syscalls only:
/// New time namespace
pub const CLONE_NEWTIME: i32 = 0x0000_0080;

/// Arguments for the clone3 syscall.
///
/// The structure is versioned by size and thus extensible.
/// New struct members must go at the end of the struct and
/// must be properly 64bit aligned.
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct clone_args_t {
    /// Flags for the new process.
    /// All flags are valid except for CSIGNAL and `CLONE_DETACHED`.
    pub flags: u64,

    /// If `CLONE_PIDFD` is set, a pidfd will be returned in this argument.
    pub pidfd: u64,

    /// If `CLONE_CHILD_SETTID` is set, the TID of the child process
    /// will be returned in the child's memory.
    pub child_tid: u64,

    /// If `CLONE_PARENT_SETTID` is set, the TID of the child process
    /// will be returned in the parent's memory.
    pub parent_tid: u64,

    /// The `exit_signal` the parent process will be sent when the child exits.
    pub exit_signal: u64,

    /// Specify the location of the stack for the child process.
    ///
    /// Note, @stack is expected to point to the lowest address.
    /// The stack direction will be determined by the kernel and
    /// set up appropriately based on `@stack_size`.
    pub stack: uintptr_t,

    /// The size of the stack for the child process.
    pub stack_size: u64,

    /// If `CLONE_SETTLS` is set, the tls descriptor is set to tls.
    pub tls: u64,

    /// Pointer to an array of type *`pid_t`.
    ///
    /// The size of the array is defined using `set_tid_size`.
    /// This array is used to select PIDs/TIDs for newly created processes.
    /// The first element in this defines the PID in the most nested PID namespace.
    /// Each additional element in the array defines the PID
    /// in the parent PID namespace of the original PID namespace. If the array has less entries
    /// than the number of currently nested PID namespaces only the PIDs in the corresponding namespaces are set.
    pub set_tid: u64,

    /// This defines the size of the array referenced in `set_tid`.
    ///
    /// This cannot be larger than the kernel's limit of nested PID namespaces.
    pub set_tid_size: u64,

    /// If `CLONE_INTO_CGROUP` is specified set this to a file descriptor for the cgroup.
    pub cgroup: u64,
}

/// sizeof first published struct
pub const CLONE_ARGS_SIZE_VER0: usize = 64;
/// sizeof second published struct
pub const CLONE_ARGS_SIZE_VER1: usize = 80;
/// sizeof third published struct
pub const CLONE_ARGS_SIZE_VER2: usize = 88;

/// Scheduling policies
pub const SCHED_NORMAL: i32 = 0;
pub const SCHED_FIFO: i32 = 1;
pub const SCHED_RR: i32 = 2;
pub const SCHED_BATCH: i32 = 3;
/// `SCHED_ISO`: reserved but not implemented yet
pub const SCHED_IDLE: i32 = 5;
pub const SCHED_DEADLINE: i32 = 6;

/// Can be `ORed` in to make sure the process is reverted back to `SCHED_NORMAL` on fork
pub const SCHED_RESET_ON_FORK: i32 = 0x4000_0000;

/// For the `sched_{set,get}attr()` calls
pub const SCHED_FLAG_RESET_ON_FORK: i32 = 0x01;
pub const SCHED_FLAG_RECLAIM: i32 = 0x02;
pub const SCHED_FLAG_DL_OVERRUN: i32 = 0x04;

pub const SCHED_FLAG_ALL: i32 =
    SCHED_FLAG_RESET_ON_FORK | SCHED_FLAG_RECLAIM | SCHED_FLAG_DL_OVERRUN;
