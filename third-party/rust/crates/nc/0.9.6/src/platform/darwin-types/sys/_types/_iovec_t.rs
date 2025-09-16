// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_iovec_t.h`

use core::ffi::c_void;

use crate::size_t;

#[repr(C)]
pub struct iovec_t {
    /// Base address of I/O memory region
    pub iov_base: *mut c_void,
    /// Size of region iov_base points to
    pub iov_len: size_t,
}
