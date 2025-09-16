// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_timespec.h`

use crate::time_t;

#[repr(C)]
#[derive(Debug, Default, Clone)]
#[allow(clippy::module_name_repetitions)]
pub struct timespec_t {
    /// seconds
    pub tv_sec: time_t,
    /// and nanoseconds
    pub tv_nsec: isize,
}
