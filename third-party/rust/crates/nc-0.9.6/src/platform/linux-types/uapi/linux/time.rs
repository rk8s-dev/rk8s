// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/time.h`

use crate::{suseconds_t, time_t};

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct timespec_t {
    /// seconds
    pub tv_sec: time_t,
    /// nanoseconds
    pub tv_nsec: isize,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct timeval_t {
    /// seconds
    pub tv_sec: time_t,
    /// microseconds
    pub tv_usec: suseconds_t,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct timezone_t {
    /// minutes west of Greenwich
    pub tz_minuteswest: i32,
    /// type of dst correction
    tz_dsttime: i32,
}

/// Names of the interval timers, and structure defining a timer setting:
pub const ITIMER_REAL: i32 = 0;
pub const ITIMER_VIRTUAL: i32 = 1;
pub const ITIMER_PROF: i32 = 2;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct itimerspec_t {
    /// timer period
    pub it_interval: timespec_t,
    /// timer expiration
    pub it_value: timespec_t,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct itimerval_t {
    /// timer interval
    pub it_interval: timeval_t,
    /// current value
    pub it_value: timeval_t,
}

/// The IDs of the various system clocks (for POSIX.1b interval timers):
pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;
pub const CLOCK_PROCESS_CPUTIME_ID: i32 = 2;
pub const CLOCK_THREAD_CPUTIME_ID: i32 = 3;
pub const CLOCK_MONOTONIC_RAW: i32 = 4;
pub const CLOCK_REALTIME_COARSE: i32 = 5;
pub const CLOCK_MONOTONIC_COARSE: i32 = 6;
pub const CLOCK_BOOTTIME: i32 = 7;
pub const CLOCK_REALTIME_ALARM: i32 = 8;
pub const CLOCK_BOOTTIME_ALARM: i32 = 9;

/// The driver implementing this got removed. The clock ID is kept as
/// a place holder. Do not reuse!
pub const CLOCK_SGI_CYCLE: i32 = 10;
pub const CLOCK_TAI: i32 = 11;

pub const MAX_CLOCKS: i32 = 16;
pub const CLOCKS_MASK: i32 = CLOCK_REALTIME | CLOCK_MONOTONIC;
pub const CLOCKS_MONO: i32 = CLOCK_MONOTONIC;

/// The various flags for setting POSIX.1b interval timers:
pub const TIMER_ABSTIME: i32 = 0x01;
