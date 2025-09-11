// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `netinet6/in6.h`

use crate::sa_family_t;
use crate::types::sys::in_port_t;

/// IPv6 address
#[repr(C)]
pub struct in6_addr_t {
    pub u6_addr: __u6_addr_un,
}

/// 128-bit IP6 address
pub union __u6_addr_un {
    __u6_addr8: [u8; 16],
    pub __u6_addr16: [u16; 8],
    __u6_addr32: [u32; 4],
}

pub const INET6_ADDRSTRLEN: usize = 46;

#[repr(C)]
pub struct sockaddr_in6_t {
    /// length of this struct
    pub sin6_len: u8,

    /// AF_INET6
    pub sin6_family: sa_family_t,

    /// Transport layer port #
    pub sin6_port: in_port_t,

    /// IP6 flow information
    pub sin6_flowinfo: u32,

    /// IP6 address
    pub sin6_addr: in6_addr_t,

    /// scope zone index
    pub sin6_scope_id: u32,
}
