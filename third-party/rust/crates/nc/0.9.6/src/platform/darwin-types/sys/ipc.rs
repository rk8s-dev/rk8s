// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/ipc.h`

use crate::{gid_t, key_t, mode_t, uid_t};

/// Information used in determining permission to perform an IPC operation
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct ipc_perm_t {
    /// Owner's user ID
    pub uid: uid_t,
    /// Owner's group ID
    pub gid: gid_t,
    /// Creator's user ID
    pub cuid: uid_t,
    /// Creator's group ID
    pub cgid: gid_t,
    /// Read/write permission
    pub mode: mode_t,
    // Reserved for internal use
    _seq: u16,
    // Reserved for internal use
    _key: key_t,
}

/// Definitions shall be provided for the following constants:
///
/// Mode bits
///
/// Create entry if key does not exist
pub const IPC_CREAT: i32 = 0o01000;
/// Fail if key exists
pub const IPC_EXCL: i32 = 0o02000;
/// Error if request must wait
pub const IPC_NOWAIT: i32 = 0o04000;

/// Keys
///
/// Private key
pub const IPC_PRIVATE: key_t = 0;

/// Control commands
///
/// Remove identifier
pub const IPC_RMID: i32 = 0;
/// Set options
pub const IPC_SET: i32 = 1;
/// Get options
pub const IPC_STAT: i32 = 2;

/// common mode bits
///
/// Read permission
pub const IPC_R: i32 = 0o00400;
/// Write/alter permission
pub const IPC_W: i32 = 0o00200;
/// Modify control info permission
pub const IPC_M: i32 = 0o10000;
