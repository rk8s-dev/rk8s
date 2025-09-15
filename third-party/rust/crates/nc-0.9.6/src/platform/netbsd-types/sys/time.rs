// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/time.h`

use crate::{suseconds_t, time_t, timespec_t};

/// Structure returned by gettimeofday(2) system call, and used in other calls.
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct timeval_t {
    /// seconds
    pub tv_sec: time_t,
    /// and microseconds
    pub tv_usec: suseconds_t,
}

/// Note: timezone is obsolete. All timezone handling is now in
/// userland. Its just here for back compatibility.
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct timezone_t {
    /// minutes west of Greenwich
    pub tz_minuteswest: i32,
    /// type of dst correction
    pub tz_dsttime: i32,
}

/// Names of the interval timers, and structure defining a timer setting.
///
/// NB: Must match the CLOCK_ constants below.
pub const ITIMER_REAL: i32 = 0;
pub const ITIMER_VIRTUAL: i32 = 1;
pub const ITIMER_PROF: i32 = 2;
pub const ITIMER_MONOTONIC: i32 = 3;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct itimerval_t {
    /// timer interval
    pub it_interval: timeval_t,
    /// current value
    pub it_value: timeval_t,
}

/// Structure defined by POSIX.1b to be like a itimerval, but with
/// timespecs. Used in the timer_*() system calls.
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct itimerspec_t {
    pub it_interval: timespec_t,
    pub it_value: timespec_t,
}

pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_VIRTUAL: i32 = 1;
pub const CLOCK_PROF: i32 = 2;
pub const CLOCK_MONOTONIC: i32 = 3;
pub const CLOCK_THREAD_CPUTIME_ID: i32 = 0x2000_0000;
pub const CLOCK_PROCESS_CPUTIME_ID: i32 = 0x4000_0000;

/// relative timer
pub const TIMER_RELTIME: i32 = 0x0;
/// absolute timer
pub const TIMER_ABSTIME: i32 = 0x1;
