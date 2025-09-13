// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_timeval32.h`

#![allow(clippy::module_name_repetitions)]

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct timeval32_t {
    /// seconds
    pub v_sec: i32,
    /// and microseconds
    pub tv_usec: i32,
}
