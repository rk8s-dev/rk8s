// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/fs_types.h`
//!
//! This is a header for the common implementation of dirent
//! to fs on-disk file type conversion.
//!
//! Although the fs on-disk bits are specific to every file system, in practice,
//! many file systems use the exact same on-disk format to describe
//! the lower 3 file type bits that represent the 7 POSIX file types.
//!
//! It is important to note that the definitions in this
//! header MUST NOT change. This would break both the
//! userspace ABI and the on-disk format of filesystems
//! using this code.
//!
//! All those file systems can use this generic code for the
//! conversions.

use crate::{mode_t, S_IFMT};

/// struct dirent file types
/// exposed to user via `getdents(2)`, `readdir(3)`
///
/// These match bits 12..15 of `stat.st_mode`
/// (ie `(i_mode >> 12) & 15`).
pub const S_DT_SHIFT: mode_t = 12;

#[must_use]
#[inline]
pub const fn s_dt(mode: mode_t) -> u8 {
    ((mode & S_IFMT) >> S_DT_SHIFT) as u8
}

#[allow(clippy::cast_possible_truncation)]
pub const S_DT_MASK: u8 = (S_IFMT >> S_DT_SHIFT) as u8;

/// these are defined by POSIX and also present in glibc's dirent.h
pub const DT_UNKNOWN: u8 = 0;
pub const DT_FIFO: u8 = 1;
pub const DT_CHR: u8 = 2;
pub const DT_DIR: u8 = 4;
pub const DT_BLK: u8 = 6;
pub const DT_REG: u8 = 8;
pub const DT_LNK: u8 = 10;
pub const DT_SOCK: u8 = 12;
pub const DT_WHT: u8 = 14;

/// 16
pub const DT_MAX: u8 = S_DT_MASK + 1;

/// fs on-disk file types.
///
/// Only the low 3 bits are used for the POSIX file types.
/// Other bits are reserved for fs private use.
/// These definitions are shared and used by multiple filesystems,
/// and MUST NOT change under any circumstances.
///
/// Note that no fs currently stores the whiteout type on-disk,
/// so whiteout dirents are exposed to user as `DT_CHR`.
pub const FT_UNKNOWN: u8 = 0;
pub const FT_REG_FILE: u8 = 1;
pub const FT_DIR: u8 = 2;
pub const FT_CHRDEV: u8 = 3;
pub const FT_BLKDEV: u8 = 4;
pub const FT_FIFO: u8 = 5;
pub const FT_SOCK: u8 = 6;
pub const FT_SYMLINK: u8 = 7;
pub const FT_MAX: u8 = 8;
