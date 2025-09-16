// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/socket.h`

use core::fmt;

/// Desired design of maximum size and alignment (see RFC2553)
/// Implementation specific max size
const K_SS_MAXSIZE: i32 = 128;

/* Implementation specific desired alignment */
//pub const _K_SS_ALIGNSIZE: i32 =(__alignof__ (struct sockaddr *))

pub type kernel_sa_family_t = u16;

#[repr(C)]
#[derive(Clone)]
pub struct kernel_sockaddr_storage_t {
    /// address family
    pub ss_family: kernel_sa_family_t,
    /// Following field(s) are implementation specific
    /// space to achieve desired size,
    /// _`SS_MAXSIZE` value minus size of `ss_family`
    pub data: [u8; (K_SS_MAXSIZE - 2) as usize],
}

// TODO(Shaohua):
//__attribute__ ((aligned(_K_SS_ALIGNSIZE)));	/* force desired alignment */
impl Default for kernel_sockaddr_storage_t {
    fn default() -> Self {
        Self {
            ss_family: 0,
            data: [0_u8; (K_SS_MAXSIZE - 2) as usize],
        }
    }
}

impl fmt::Debug for kernel_sockaddr_storage_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("kernel_sockaddr_storage_t")
            .field("ss_family", &self.ss_family)
            .field("data", &&self.data[0..32])
            .finish()
    }
}
