// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/syslimits.h`

use crate::{gid_t, uid_t};

/// max bytes for an exec function
pub const ARG_MAX: usize = 256 * 1024;
/// max simultaneous processes
pub const CHILD_MAX: usize = 160;
/// max value for a gid_t (2^31-2)
pub const GID_MAX: gid_t = 21_4748_3647;
/// max file link count
pub const LINK_MAX: usize = 32767;
/// max bytes in term canon input line
pub const MAX_CANON: usize = 255;
/// max bytes in terminal input
pub const MAX_INPUT: usize = 255;
/// max bytes in a file name, must be
// kept in sync with MAXNAMLEN
pub const NAME_MAX: usize = 511;
/// max supplemental group id's
pub const NGROUPS_MAX: usize = 16;
/// max value for a uid_t (2^31-2)
pub const UID_MAX: uid_t = 21_4748_3647;
/// max open files per process
pub const OPEN_MAX: usize = 128;
/// max bytes in pathname
pub const PATH_MAX: usize = 1024;
/// max bytes for atomic pipe writes
pub const PIPE_BUF: usize = 512;

/// max ibase/obase values in bc(1)
pub const BC_BASE_MAX: i32 = i32::MAX;
/// max array elements in bc(1)
pub const BC_DIM_MAX: i32 = 65535;
/// max scale value in bc(1)
pub const BC_SCALE_MAX: i32 = i32::MAX;
/// max const string length in bc(1)
pub const BC_STRING_MAX: i32 = i32::MAX;
/// max weights for order keyword
pub const COLL_WEIGHTS_MAX: i32 = 2;
/// max expressions nested in expr(1)
pub const EXPR_NEST_MAX: i32 = 32;
/// max bytes in an input line
pub const LINE_MAX: i32 = 2048;
/// max RE's in interval notation
pub const RE_DUP_MAX: i32 = 255;

/// max login name length incl. NUL
pub const LOGIN_NAME_MAX: usize = 17;

/// max # of iovec's for readv(2) etc.
pub const IOV_MAX: usize = 1024;
/// default "nice"
pub const NZERO: i32 = 20;
