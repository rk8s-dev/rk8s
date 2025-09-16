// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_iovec.h`

use core::ffi::c_void;
use core::ptr;

use crate::size_t;

#[repr(C)]
#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct iovec_t {
    /// Base address.
    pub iov_base: *const c_void,

    /// Length.
    pub iov_len: size_t,
}

impl Default for iovec_t {
    fn default() -> Self {
        Self {
            iov_base: ptr::null(),
            iov_len: 0,
        }
    }
}
