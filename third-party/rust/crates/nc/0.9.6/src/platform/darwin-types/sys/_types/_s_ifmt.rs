// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_s_ifmt.h`

/// The symbolic names for file modes for use as values of `mode_t`
/// shall be defined as described in `<sys/stat.h>`

/// File type
///
/// type of file mask
pub const S_IFMT: i32 = 0o170_000;
/// named pipe (fifo)
pub const S_IFIFO: i32 = 0o010_000;
/// character special
pub const S_IFCHR: i32 = 0o020_000;
/// directory
pub const S_IFDIR: i32 = 0o040_000;
/// block special
pub const S_IFBLK: i32 = 0o060_000;
/// regular
pub const S_IFREG: i32 = 0o100_000;
/// symbolic link
pub const S_IFLNK: i32 = 0o120_000;
/// socket
pub const S_IFSOCK: i32 = 0o140_000;
/// OBSOLETE: whiteout
pub const S_IFWHT: i32 = 0o160_000;

/// File mode
///
/// Read, write, execute/search by owner RWX mask for owner
pub const S_IRWXU: i32 = 0o000_700;
/// R for owner
pub const S_IRUSR: i32 = 0o000_400;
/// W for owner
pub const S_IWUSR: i32 = 0o000_200;
/// X for owner
pub const S_IXUSR: i32 = 0o000_100;
/// Read, write, execute/search by group
/// RWX mask for group
pub const S_IRWXG: i32 = 0o000_070;
/// R for group
pub const S_IRGRP: i32 = 0o000_040;
/// W for group
pub const S_IWGRP: i32 = 0o000_020;
/// X for group
pub const S_IXGRP: i32 = 0o000_010;
/// Read, write, execute/search by others
/// RWX mask for other
pub const S_IRWXO: i32 = 0o000_007;
/// R for other
pub const S_IROTH: i32 = 0o000_004;
/// W for other
pub const S_IWOTH: i32 = 0o000_002;
/// X for other
pub const S_IXOTH: i32 = 0o000_001;

/// set user id on execution
pub const S_ISUID: i32 = 0o004_000;
/// set group id on execution
pub const S_ISGID: i32 = 0o002_000;
/// directory restrcted delete
pub const S_ISVTX: i32 = 0o001_000;

/// sticky bit: not supported
pub const S_ISTXT: i32 = S_ISVTX;
/// backward compatability
pub const S_IREAD: i32 = S_IRUSR;
/// backward compatability
pub const S_IWRITE: i32 = S_IWUSR;
/// backward compatability
pub const S_IEXEC: i32 = S_IXUSR;
