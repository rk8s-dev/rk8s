// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_timeval.h`

use crate::{suseconds_t, time_t};

/// Structure returned by `gettimeofday(2)` system call, and used in other calls.
#[repr(C)]
#[derive(Debug, Default, Clone)]
#[allow(clippy::module_name_repetitions)]
pub struct timeval_t {
    /// seconds
    pub tv_sec: time_t,
    /// and microseconds
    pub tv_usec: suseconds_t,
}
