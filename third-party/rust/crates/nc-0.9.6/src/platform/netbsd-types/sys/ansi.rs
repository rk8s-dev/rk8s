// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/ansi.h`

use core::ffi::c_void;

/// core address
pub type caddr_t = *mut c_void;
/// group id
pub type gid_t = u32;
/// IP(v4) address
pub type in_addr_t = u32;
/// "Internet" port number
pub type in_port_t = u16;
/// file permissions
pub type mode_t = u32;
/// file offset
pub type off_t = i64;
/// process id
pub type pid_t = i32;
/* socket address family */
pub type sa_family_t = u8;
/// socket-related datum length
pub type socklen_t = u32;
/// user id
pub type uid_t = u32;
/// fs block count (statvfs)
pub type fsblkcnt_t = u64;
/// fs file count
pub type fsfilcnt_t = u64;
