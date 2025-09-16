// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/quota.h`

use crate::{id_t, time_t};

/// quota id types (entities being billed)
pub const QUOTA_IDTYPE_USER: id_t = 0;
pub const QUOTA_IDTYPE_GROUP: id_t = 1;

/// quota object types (things being limited)
pub const QUOTA_OBJTYPE_BLOCKS: id_t = 0;
pub const QUOTA_OBJTYPE_FILES: id_t = 1;

/// id value for "default"
pub const QUOTA_DEFAULTID: id_t = -1_i32 as id_t;

/// limit value for "no limit"
pub const QUOTA_NOLIMIT: u64 = 0xffff_ffff_ffff_ffff;

/// time value for "no time"
pub const QUOTA_NOTIME: time_t = -1;

/// Semantic restrictions.
///
/// These are hints applications can use to help produce comprehensible error diagnostics
/// when something unsupported is attempted.
/// quotacheck(8) required
pub const QUOTA_RESTRICT_NEEDSQUOTACHECK: i32 = 0x1;
/// grace time is global
pub const QUOTA_RESTRICT_UNIFORMGRACE: i32 = 0x2;
/// values limited to 2^32
pub const QUOTA_RESTRICT_32BIT: i32 = 0x4;
/// updates not supported
pub const QUOTA_RESTRICT_READONLY: i32 = 0x8;

/// Structure used to describe the key part of a quota record.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct quotakey_t {
    /// type of id (user, group, etc.)
    pub qk_idtype: i32,

    /// actual id number
    pub qk_id: id_t,

    /// type of fs object (blocks, files, etc.)
    pub qk_objtype: i32,
}

/// Structure used to describe the value part of a quota record.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct quotaval_t {
    /// absolute limit
    pub qv_hardlimit: u64,

    /// overflowable limit
    pub qv_softlimit: u64,

    /// current usage
    pub qv_usage: u64,

    /// time when softlimit grace expires
    pub qv_expiretime: time_t,

    /// allowed time for overflowing soft limit
    pub qv_grace: time_t,
}
