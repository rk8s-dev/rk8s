// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/uio.h`

use core::ffi::c_void;
use core::ptr;

use crate::size_t;

/// Berkeley style UIO structures
#[repr(C)]
#[derive(Debug, Clone)]
pub struct iovec_t {
    /// BSD uses `caddr_t` (1003.1g requires void *)
    pub iov_base: *const c_void,
    /// Must be `size_t` (1003.1g)
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

/// `UIO_MAXIOV` shall be at least 16 1003.1g (5.4.1.1)
pub const UIO_FASTIOV: i32 = 8;
pub const UIO_MAXIOV: i32 = 1024;
