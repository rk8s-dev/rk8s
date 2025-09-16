// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_uio.h`

#[repr(C)]
#[derive(Debug, Clone)]
pub enum uio_rw_t {
    UIO_READ,
    UIO_WRITE,
}

impl Default for uio_rw_t {
    fn default() -> Self {
        Self::UIO_READ
    }
}

/// Segment flag values.
#[repr(C)]
#[derive(Debug, Clone)]
pub enum uio_seg_t {
    /// from user data space
    UIO_USERSPACE,

    /// from system space
    UIO_SYSSPACE,

    /// don't copy, already in object
    UIO_NOCOPY,
}

impl Default for uio_seg_t {
    fn default() -> Self {
        Self::UIO_USERSPACE
    }
}
