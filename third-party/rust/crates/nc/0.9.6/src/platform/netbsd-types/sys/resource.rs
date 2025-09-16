// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/resource.h`

use crate::{fixpt_t, rlim_t, timeval_t};

/// Process priority specifications to get/setpriority.
pub const PRIO_MIN: i32 = -20;
pub const PRIO_MAX: i32 = 20;

pub const PRIO_PROCESS: i32 = 0;
pub const PRIO_PGRP: i32 = 1;
pub const PRIO_USER: i32 = 2;

/// Resource utilization information.
pub const RUSAGE_SELF: i32 = 0;
pub const RUSAGE_CHILDREN: i32 = -1;

#[derive(Debug, Default, Clone)]
#[repr(C)]
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

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct wrusage_t {
    pub wru_self: rusage_t,
    pub wru_children: rusage_t,
}

/// Resource limits
///
/// cpu time in milliseconds
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
/// virtual process size (inclusive of mmap)
pub const RLIMIT_AS: i32 = 10;
/// common alias
pub const RLIMIT_VMEM: i32 = RLIMIT_AS;
/// number of threads
pub const RLIMIT_NTHR: i32 = 11;

/// number of resource limits
pub const RLIM_NLIMITS: i32 = 12;

/// no limit
#[allow(clippy::cast_possible_wrap)]
pub const RLIM_INFINITY: rlim_t = !(1_u64 << 63) as rlim_t;
/// unrepresentable hard limit
pub const RLIM_SAVED_MAX: rlim_t = RLIM_INFINITY;
/// unrepresentable soft limit
pub const RLIM_SAVED_CUR: rlim_t = RLIM_INFINITY;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct rlimit_t {
    /// current (soft) limit
    pub rlim_cur: rlim_t,
    /// maximum value for `rlim_cur`
    pub rlim_max: rlim_t,
}

/// Load average structure.
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct loadavg_t {
    pub ldavg: [fixpt_t; 3],
    pub fscale: isize,
}
