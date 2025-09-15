// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/time.h`

use crate::timeval_t;

/// Structure used as a parameter by getitimer(2) and setitimer(2) system calls.
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct itimerval_t {
    /// timer interval
    pub it_interval: timeval_t,
    /// current value
    pub it_value: timeval_t,
}

/// Names of the interval timers, and structure defining a timer setting.
pub const ITIMER_REAL: i32 = 0;
pub const ITIMER_VIRTUAL: i32 = 1;
pub const ITIMER_PROF: i32 = 2;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct timezone_t {
    /// minutes west of Greenwich
    pub tz_minuteswest: i32,
    /// type of dst correction
    pub tz_dsttime: i32,
}

/// not on dst
pub const DST_NONE: i32 = 0;
/// USA style dst
pub const DST_USA: i32 = 1;
/// Australian style dst
pub const DST_AUST: i32 = 2;
/// Western European dst
pub const DST_WET: i32 = 3;
/// Middle European dst
pub const DST_MET: i32 = 4;
/// Eastern European dst
pub const DST_EET: i32 = 5;
/// Canada
pub const DST_CAN: i32 = 6;

/// Getkerninfo clock information structure
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct clockinfo_t {
    /// clock frequency
    pub hz: i32,
    /// micro-seconds per hz tick
    pub tick: i32,
    /// clock skew rate for adjtime()
    pub tickadj: i32,
    /// statistics clock frequency
    pub stathz: i32,
    /// profiling clock frequency
    pub profhz: i32,
}
