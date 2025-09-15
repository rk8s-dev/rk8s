// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/quota.h`

/// element used for user quotas
const USRQUOTA: i32 = 0;
/// element used for group quotas
const GRPQUOTA: i32 = 1;
/// element used for project quotas
const PRJQUOTA: i32 = 2;

/// Masks for quota types when used as a bitmask
pub const QTYPE_MASK_USR: i32 = 1 << USRQUOTA;
pub const QTYPE_MASK_GRP: i32 = 1 << GRPQUOTA;
pub const QTYPE_MASK_PRJ: i32 = 1 << PRJQUOTA;

/// Type in which we store ids in memory
pub type qid_t = i32;

/// Type in which we store sizes
pub type qsize_t = i64;

// Other types are ignored!
