// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/limits.h`

pub const NR_OPEN: i32 = 1024;

/// supplemental group IDs are available
pub const NGROUPS_MAX: i32 = 65536;
/// # bytes of args + environ for `exec()`
pub const ARG_MAX: i32 = 131_072;
/// # links a file may have
pub const LINK_MAX: i32 = 127;
/// size of the canonical input queue
pub const MAX_CANON: i32 = 255;
/// size of the type-ahead buffer
pub const MAX_INPUT: i32 = 255;
/// # chars in a file name
pub const NAME_MAX: i32 = 255;
/// # chars in a path name including nul
pub const PATH_MAX: i32 = 4096;
/// # bytes in atomic write to a pipe
pub const PIPE_BUF: i32 = 4096;
/// # chars in an extended attribute name
pub const XATTR_NAME_MAX: i32 = 255;
/// size of an extended attribute value (64k)
pub const XATTR_SIZE_MAX: i32 = 65536;
/// size of extended attribute namelist (64k)
pub const XATTR_LIST_MAX: i32 = 65536;

pub const RTSIG_MAX: i32 = 32;
