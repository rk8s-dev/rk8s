// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/time.h`

use crate::{time_t, timeval_t};

#[repr(C)]
#[derive(Debug, Default, Clone)]
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

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct bintime_t {
    pub sec: time_t,
    pub frac: u64,
}

/// Names of the interval timers, and structure defining a timer setting.
pub const ITIMER_REAL: i32 = 0;
pub const ITIMER_VIRTUAL: i32 = 1;
pub const ITIMER_PROF: i32 = 2;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct itimerval_t {
    /// timer interval
    pub it_interval: timeval_t,

    /// current value
    pub it_value: timeval_t,
}
