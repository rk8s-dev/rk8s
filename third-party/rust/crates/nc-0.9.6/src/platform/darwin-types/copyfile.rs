// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `copyfile.h`

pub type copyfile_flags_t = u32;

pub const COPYFILE_STATE_SRC_FD: i32 = 1;
pub const COPYFILE_STATE_SRC_FILENAME: i32 = 2;
pub const COPYFILE_STATE_DST_FD: i32 = 3;
pub const COPYFILE_STATE_DST_FILENAME: i32 = 4;
pub const COPYFILE_STATE_QUARANTINE: i32 = 5;
pub const COPYFILE_STATE_STATUS_CB: i32 = 6;
pub const COPYFILE_STATE_STATUS_CTX: i32 = 7;
pub const COPYFILE_STATE_COPIED: i32 = 8;
pub const COPYFILE_STATE_XATTRNAME: i32 = 9;
pub const COPYFILE_STATE_WAS_CLONED: i32 = 10;

pub const COPYFILE_DISABLE_VAR: &str = "COPYFILE_DISABLE";

/// flags for copyfile
pub const COPYFILE_ACL: copyfile_flags_t = 1 << 0;
pub const COPYFILE_STAT: copyfile_flags_t = 1 << 1;
pub const COPYFILE_XATTR: copyfile_flags_t = 1 << 2;
pub const COPYFILE_DATA: copyfile_flags_t = 1 << 3;

pub const COPYFILE_SECURITY: copyfile_flags_t = COPYFILE_STAT | COPYFILE_ACL;
pub const COPYFILE_METADATA: copyfile_flags_t = COPYFILE_SECURITY | COPYFILE_XATTR;
pub const COPYFILE_ALLA: copyfile_flags_t = COPYFILE_METADATA | COPYFILE_DATA;

/// Descend into hierarchies
pub const COPYFILE_RECURSIVE: copyfile_flags_t = 1 << 15;
/// return flags for xattr or acls if set
pub const COPYFILE_CHECK: copyfile_flags_t = 1 << 16;
/// fail if destination exists
pub const COPYFILE_EXCL: copyfile_flags_t = 1 << 17;
/// don't follow if source is a symlink
pub const COPYFILE_NOFOLLOW_SRC: copyfile_flags_t = 1 << 18;
/// don't follow if dst is a symlink
pub const COPYFILE_NOFOLLOW_DST: copyfile_flags_t = 1 << 19;
/// unlink src after copy
pub const COPYFILE_MOVE: copyfile_flags_t = 1 << 20;
/// unlink dst before copy
pub const COPYFILE_UNLINK: copyfile_flags_t = 1 << 21;
pub const COPYFILE_NOFOLLOW: copyfile_flags_t = COPYFILE_NOFOLLOW_SRC | COPYFILE_NOFOLLOW_DST;

pub const COPYFILE_PACK: copyfile_flags_t = 1 << 22;
pub const COPYFILE_UNPACK: copyfile_flags_t = 1 << 23;

pub const COPYFILE_CLONE: copyfile_flags_t = 1 << 24;
pub const COPYFILE_CLONE_FORCE: copyfile_flags_t = 1 << 25;

pub const COPYFILE_RUN_IN_PLACE: copyfile_flags_t = 1 << 26;
pub const COPYFILE_DATA_SPARSE: copyfile_flags_t = 1 << 27;
pub const COPYFILE_PRESERVE_DST_TRACKED: copyfile_flags_t = 1 << 28;
pub const COPYFILE_VERBOSE: copyfile_flags_t = 1 << 30;

pub const COPYFILE_RECURSE_ERROR: i32 = 0;
pub const COPYFILE_RECURSE_FILE: i32 = 1;
pub const COPYFILE_RECURSE_DIR: i32 = 2;
pub const COPYFILE_RECURSE_DIR_CLEANUP: i32 = 3;
pub const COPYFILE_COPY_DATA: i32 = 4;
pub const COPYFILE_COPY_XATTR: i32 = 5;

pub const COPYFILE_START: i32 = 1;
pub const COPYFILE_FINISH: i32 = 2;
pub const COPYFILE_ERR: i32 = 3;
pub const COPYFILE_PROGRESS: i32 = 4;

pub const COPYFILE_CONTINUE: i32 = 0;
pub const COPYFILE_SKIP: i32 = 1;
pub const COPYFILE_QUIT: i32 = 2;
