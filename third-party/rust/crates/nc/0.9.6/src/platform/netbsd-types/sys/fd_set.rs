// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/fd_set.h`
//!
//!  Implementation dependent defines, hidden from user space.
//!  POSIX does not specify them.

use core::mem::size_of;

pub type __fd_mask_t = u32;

/// 32 = 2 ^ 5
pub const __NFDBITS: i32 = 32;
pub const __NFDSHIFT: i32 = 5;
pub const __NFDMASK: i32 = __NFDBITS - 1;

/// Select uses bit fields of file descriptors.
/// These macros manipulate such bit fields.
///
/// Note: FD_SETSIZE may be defined by the user.
pub const FD_SETSIZE: i32 = 256;

#[must_use]
pub const fn __NFD_LEN(a: i32) -> usize {
    ((a + (__NFDBITS - 1)) / __NFDBITS) as usize
}

pub const __NFD_SIZE: usize = __NFD_LEN(FD_SETSIZE);

#[must_use]
pub const fn __NFD_BYTES(a: i32) -> usize {
    __NFD_LEN(a) * size_of::<__fd_mask_t>()
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct fd_set_t {
    pub fds_bits: [__fd_mask_t; __NFD_SIZE],
}

/// Expose our internals if we are not required to hide them.
pub type fd_mask_t = __fd_mask_t;
pub const NFDBITS: i32 = __NFDBITS;
