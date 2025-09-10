// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/sched.h`

#![allow(clippy::module_name_repetitions)]

/// `sched_add` arguments (formerly setrunqueue)
/// No special circumstances.
pub const SRQ_BORING: i32 = 0x0000;
/// We are yielding (from `mi_switch`).
pub const SRQ_YIELDING: i32 = 0x0001;
/// It is ourself (from `mi_switch`).
pub const SRQ_OURSELF: i32 = 0x0002;
/// It is probably urgent.
pub const SRQ_INTR: i32 = 0x0004;
/// has been preempted.. be kind
pub const SRQ_PREEMPTED: i32 = 0x0008;
/// Priority updated due to `prio_lend`
pub const SRQ_BORROWING: i32 = 0x0010;
/// Return holding original td lock
pub const SRQ_HOLD: i32 = 0x0020;
/// Return holding td lock
pub const SRQ_HOLDTD: i32 = 0x0040;

/// POSIX 1003.1b Process Scheduling
///
/// POSIX scheduling policies
pub const SCHED_FIFO: i32 = 1;
pub const SCHED_OTHER: i32 = 2;
pub const SCHED_RR: i32 = 3;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct sched_param_t {
    pub sched_priority: i32,
}
