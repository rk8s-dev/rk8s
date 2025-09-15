// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/timespec.h`

use crate::timespec_t;

/// Structure defined by POSIX.1b to be like a itimerval, but with
/// timespecs.
///
/// Used in the `timer_*()` system calls.
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct itimerspec_t {
    pub it_interval: timespec_t,
    pub it_value: timespec_t,
}
