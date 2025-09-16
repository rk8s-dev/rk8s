// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/jail.h`

use crate::{c_char, cpusetid_t, in6_addr_t, in_addr_t, MAXHOSTNAMELEN, MAXPATHLEN};

#[repr(C)]
#[derive(Debug, Clone)]
pub struct jail_t {
    pub version: u32,
    pub path: *const c_char,
    pub hostname: *const c_char,
    pub jailname: *const c_char,
    pub ip4s: u32,
    pub ip6s: u32,
    pub ip4: *const in_addr_t,
    pub ip6: *const in6_addr_t,
}

pub const JAIL_API_VERSION: i32 = 2;

#[repr(C)]
pub struct xprison_t {
    pub pr_version: i32,
    pub pr_id: i32,
    pub pr_state: i32,
    pub pr_cpusetid: cpusetid_t,
    pub pr_path: [c_char; MAXPATHLEN],
    pub pr_host: [c_char; MAXHOSTNAMELEN],
    pub pr_name: [c_char; MAXHOSTNAMELEN],
    pub pr_ip4s: u32,
    pub pr_ip6s: u32,
}
pub const XPRISON_VERSION: i32 = 3;

pub enum prison_state_e {
    /// New prison, not ready to be seen
    PRISON_STATE_INVALID = 0,

    /// Current prison, visible to all
    PRISON_STATE_ALIVE,

    /// Removed but holding resources
    PRISON_STATE_DYING,
}

/// Flags for jail_set and jail_get.
/// Create jail if it doesn't exist
pub const JAIL_CREATE: i32 = 0x01;
/// Update parameters of existing jail
pub const JAIL_UPDATE: i32 = 0x02;
/// Attach to jail upon creation
pub const JAIL_ATTACH: i32 = 0x04;
/// Allow getting a dying jail
pub const JAIL_DYING: i32 = 0x08;
pub const JAIL_SET_MASK: i32 = 0x0f;
pub const JAIL_GET_MASK: i32 = 0x08;

pub const JAIL_SYS_DISABLE: i32 = 0;
pub const JAIL_SYS_NEW: i32 = 1;
pub const JAIL_SYS_INHERIT: i32 = 2;

pub const HOSTUUIDLEN: usize = 64;
pub const DEFAULT_HOSTUUID: &str = "00000000-0000-0000-0000-000000000000";
pub const OSRELEASELEN: usize = 32;

#[repr(C)]
pub enum pr_family_e {
    PR_INET = 0,
    PR_INET6 = 1,
    PR_FAMILY_MAX = 2,
}
