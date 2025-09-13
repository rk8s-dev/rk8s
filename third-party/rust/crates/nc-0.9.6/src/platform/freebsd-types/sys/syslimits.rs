// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/syslimits.h`

#[cfg(target_pointer_width = "32")]
/// max bytes for an exec function
pub const ARG_MAX: usize = 2 * 256 * 1024;

#[cfg(target_pointer_width = "64")]
/// max bytes for KVA-starved archs
pub const ARG_MAX: usize = 256 * 1024;

/// max simultaneous processes
pub const CHILD_MAX: usize = 40;

/// max bytes in term canon input line
pub const MAX_CANON: usize = 255;
/// max bytes in terminal input
pub const MAX_INPUT: usize = 255;
/// max bytes in a file name
pub const NAME_MAX: usize = 255;
/// max supplemental group id's
pub const NGROUPS_MAX: usize = 1023;

/// max open files per process
pub const OPEN_MAX: usize = 64;

/// max bytes in pathname
pub const PATH_MAX: usize = 1024;
/// max bytes for atomic pipe writes
pub const PIPE_BUF: usize = 512;
/// max elements in i/o vector
pub const IOV_MAX: usize = 1024;
