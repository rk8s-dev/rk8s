// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/select.h`

pub type fd_mask_t = usize;

/// Select uses bit masks of file descriptors in longs.  These macros
/// manipulate such bit fields (the filesystem macros use chars).
/// `FD_SETSIZE` may be defined by the user, but the default here should
/// be enough for most uses.
pub const FD_SETSIZE: usize = 1024;

/// bits per mask
pub const NFDBITS: usize = fd_mask_t::BITS as usize;

const fn howmany(x: usize, y: usize) -> usize {
    (x + y - 1) / y
}

#[repr(C)]
#[derive(Debug)]
pub struct fd_set_t {
    pub fds_bits: [fd_mask_t; howmany(FD_SETSIZE, NFDBITS)],
}

impl Default for fd_set_t {
    fn default() -> Self {
        Self {
            fds_bits: [0; howmany(FD_SETSIZE, NFDBITS)],
        }
    }
}
