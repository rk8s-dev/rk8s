// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_fd_def.h`

use core::mem::size_of;

/// Select uses bit masks of file descriptors in longs.  These macros
/// manipulate such bit fields (the filesystem macros use chars).  The
/// extra protection here is to permit application redefinition above
/// the default size.
pub const __DARWIN_FD_SETSIZE: usize = 1024;

/// bits in a byte
pub const __DARWIN_NBBY: usize = 8;

/// bits per mask
pub const __DARWIN_NFDBITS: usize = size_of::<i32>() * __DARWIN_NBBY;

#[must_use]
pub const fn __DARWIN_howmany(x: usize, y: usize) -> usize {
    // # y's == x bits?
    if (x % y) == 0 {
        x / y
    } else {
        (x / y) + 1
    }
}

pub struct fd_set_t {
    pub fds_bits: [i32; __DARWIN_howmany(__DARWIN_FD_SETSIZE, __DARWIN_NFDBITS)],
}
