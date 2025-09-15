// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/resources.h`

use crate::{rlim_t, timeval_t};

/// Process priority specifications to get/setpriority.
pub const PRIO_MIN: i32 = -20;
pub const PRIO_MAX: i32 = 20;

pub const PRIO_PROCESS: i32 = 0;
pub const PRIO_PGRP: i32 = 1;
pub const PRIO_USER: i32 = 2;

/// Resource utilization information.
///
/// All fields are only modified by curthread and no locks are required to read.

pub const RUSAGE_SELF: i32 = 0;
pub const RUSAGE_CHILDREN: i32 = -1;
pub const RUSAGE_THREAD: i32 = 1;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct rusage_t {
    /// user time used
    pub ru_utime: timeval_t,

    /// system time used
    pub ru_stime: timeval_t,

    /// max resident set size
    pub ru_maxrss: isize,

    /// integral shared memory size
    pub ru_ixrss: isize,

    /// integral unshared data
    pub ru_idrss: isize,

    /// integral unshared stack
    pub ru_isrss: isize,

    /// page reclaims
    pub ru_minflt: isize,

    /// page faults
    pub ru_majflt: isize,

    /// swaps
    pub ru_nswap: isize,

    /// block input operations
    pub ru_inblock: isize,

    /// block output operations
    pub ru_oublock: isize,

    /// messages sent
    pub ru_msgsnd: isize,

    /// messages received
    pub ru_msgrcv: isize,

    /// signals received
    pub ru_nsignals: isize,

    /// voluntary context switches
    pub ru_nvcsw: isize,

    /// involuntary
    pub ru_nivcsw: isize,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct wrusage_t {
    pub wru_self: rusage_t,
    pub wru_children: rusage_t,
}

/// Resource limits
/// maximum cpu time in seconds
pub const RLIMIT_CPU: i32 = 0;
/// maximum file size
pub const RLIMIT_FSIZE: i32 = 1;
/// data size
pub const RLIMIT_DATA: i32 = 2;
/// stack size
pub const RLIMIT_STACK: i32 = 3;
/// core file size
pub const RLIMIT_CORE: i32 = 4;
/// resident set size
pub const RLIMIT_RSS: i32 = 5;
/// locked-in-memory address space
pub const RLIMIT_MEMLOCK: i32 = 6;
/// number of processes
pub const RLIMIT_NPROC: i32 = 7;
/// number of open files
pub const RLIMIT_NOFILE: i32 = 8;
/// maximum size of all socket buffers
pub const RLIMIT_SBSIZE: i32 = 9;
/// virtual process size (incl. mmap)
pub const RLIMIT_VMEM: i32 = 10;
/// standard name for `RLIMIT_VMEM`
pub const RLIMIT_AS: i32 = RLIMIT_VMEM;
/// pseudo-terminals
pub const RLIMIT_NPTS: i32 = 11;
/// swap used
pub const RLIMIT_SWAP: i32 = 12;
/// kqueues allocated
pub const RLIMIT_KQUEUES: i32 = 13;
/// process-shared umtx
pub const RLIMIT_UMTXP: i32 = 14;

/// number of resource limits
pub const RLIM_NLIMITS: i32 = 15;

#[allow(clippy::cast_possible_wrap)]
pub const RLIM_INFINITY: rlim_t = ((1_u64 << 63) - 1) as rlim_t;
pub const RLIM_SAVED_MAX: rlim_t = RLIM_INFINITY;
pub const RLIM_SAVED_CUR: rlim_t = RLIM_INFINITY;

/// Resource limit string identifiers
#[repr(C)]
#[derive(Debug, Default)]
pub struct rlimit_t {
    /// current (soft) limit
    pub rlim_cur: rlim_t,

    /// maximum value for rlim_cur
    pub rlim_max: rlim_t,
}

pub const CP_USER: i32 = 0;
pub const CP_NICE: i32 = 1;
pub const CP_SYS: i32 = 2;
pub const CP_INTR: i32 = 3;
pub const CP_IDLE: i32 = 4;
pub const CPUSTATES: i32 = 5;
