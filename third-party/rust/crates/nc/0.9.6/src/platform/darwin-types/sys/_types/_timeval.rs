// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_timeval.h`

#![allow(clippy::module_name_repetitions)]

use crate::{__darwin_time_t, suseconds_t};

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct timeval_t {
    /// seconds
    pub tv_sec: __darwin_time_t,
    /// and microseconds
    pub tv_usec: suseconds_t,
}
