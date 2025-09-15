// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/syslimits.h`

use crate::{gid_t, uid_t};

/// max bytes for an exec function
pub const ARG_MAX: usize = 1024 * 1024;

/// max simultaneous processes
pub const CHILD_MAX: usize = 266;
/// max value for a gid_t (2^31-2)
pub const GID_MAX: gid_t = 2147483647;
/// max file link count
pub const LINK_MAX: usize = 32767;
/// max bytes in term canon input line
pub const MAX_CANON: usize = 1024;
/// max bytes in terminal input
pub const MAX_INPUT: usize = 1024;
/// max bytes in a file name
pub const NAME_MAX: usize = 255;
/// max supplemental group id's
pub const NGROUPS_MAX: usize = 16;
/// max value for a uid_t (2^31-2)
pub const UID_MAX: uid_t = 2147483647;

/// max open files per process - todo, make a config option?
pub const OPEN_MAX: usize = 10240;

/// max bytes in pathname
pub const PATH_MAX: usize = 1024;
/// max bytes for atomic pipe writes
pub const PIPE_BUF: usize = 512;

/// max ibase/obase values in bc(1)
pub const BC_BASE_MAX: usize = 99;
/// max array elements in bc(1)
pub const BC_DIM_MAX: usize = 2048;
/// max scale value in bc(1)
pub const BC_SCALE_MAX: usize = 99;
/// max const string length in bc(1)
pub const BC_STRING_MAX: usize = 1000;
/// max character class name size
pub const CHARCLASS_NAME_MAX: usize = 14;
/// max weights for order keyword
pub const COLL_WEIGHTS_MAX: usize = 2;
pub const EQUIV_CLASS_MAX: usize = 2;
/// max expressions nested in expr(1)
pub const EXPR_NEST_MAX: usize = 32;
/// max bytes in an input line
pub const LINE_MAX: usize = 2048;
/// max RE's in interval notation
pub const RE_DUP_MAX: usize = 255;

/// default priority
///
/// range: -20 - 20
/// (PRIO_MIN - PRIO_MAX)
pub const NZERO: i32 = 0;
