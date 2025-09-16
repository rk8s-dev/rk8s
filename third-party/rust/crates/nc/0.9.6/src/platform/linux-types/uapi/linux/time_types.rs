// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/time_types.h`

use crate::{time64_t, timespec_t};

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct kernel_timespec_t {
    /// seconds
    pub tv_sec: time64_t,
    /// nanoseconds
    pub tv_nsec: i64,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct kernel_itimerspec_t {
    /// timer period
    pub it_interval: timespec_t,
    /// timer expiration
    pub it_value: timespec_t,
}

/// legacy timeval structure, only embedded in structures that
/// traditionally used 'timeval' to pass time intervals (not absolute times).
///
/// Do not add new users. If user space fails to compile here,
/// this is probably because it is not y2038 safe and needs to
/// be changed to use another interface.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct kernle_old_timeval_t {
    pub tv_sec: isize,
    pub tv_usec: isize,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct kernel_sock_timeval_t {
    pub tv_sec: i64,
    pub tv_usec: i64,
}
