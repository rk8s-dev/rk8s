// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/uio.h`

#![allow(clippy::module_name_repetitions)]

use core::ffi::c_void;

use crate::{off_t, size_t};

#[repr(C)]
pub struct iovec_t {
    /// Base address.
    pub iov_base: *mut c_void,
    /// Length.
    pub iov_len: size_t,
}

#[repr(C)]
pub enum uio_rw_t {
    UIO_READ,
    UIO_WRITE,
}

/// Segment flag values.
#[repr(C)]
pub enum uio_seg_t {
    /// from user data space
    UIO_USERSPACE,
    /// from system space
    UIO_SYSSPACE,
}

#[repr(C)]
pub struct uio_t {
    /// pointer to array of iovecs
    pub uio_iov: *mut iovec_t,
    /// number of iovecs in array
    pub uio_iovcnt: i32,
    /// offset into file this uio corresponds to
    pub uio_offset: off_t,
    /// residual i/o count
    pub uio_resid: size_t,
    pub uio_rw: uio_rw_t,
    // TODO(Shaohua):
    //pub uio_vmspace: *mut vmspace_t,
    pub uio_vmspace: *mut c_void,
}

/// Limits
///
/// Deprecated: use `IOV_MAX` from <limits.h> instead.
/// max 1K of iov's
pub const UIO_MAXIOV: i32 = 1024;

/// 8 on stack, more will be dynamically allocated.
pub const UIO_SMALLIOV: i32 = 8;
