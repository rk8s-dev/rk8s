// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/ipc.h`

use crate::{gid_t, key_t, mode_t, uid_t};

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct ipc_perm_t {
    /// creator user id
    pub cuid: uid_t,
    /// creator group id
    pub cgid: gid_t,
    /// user id
    pub uid: uid_t,
    /// group id
    pub gid: gid_t,
    /// r/w permission
    pub mode: mode_t,
    /// sequence # (to generate unique ipcid)
    pub seq: u16,
    /// user specified msg/sem/shm key
    pub key: key_t,
}

/// common mode bits
///
/// read permission
pub const IPC_R: i32 = 0o00_400;
/// write/alter permission
pub const IPC_W: i32 = 0o00_200;
/// permission to change control info
pub const IPC_M: i32 = 0o10_000;

/// SVID required constants (same values as system 5)
/// create entry if key does not exist
pub const IPC_CREAT: i32 = 0o01_000;
/// fail if key exists
pub const IPC_EXCL: i32 = 0o02_000;
/// error if request must wait
pub const IPC_NOWAIT: i32 = 0o04_000;

/// private key
pub const IPC_PRIVATE: key_t = 0;

/// remove identifier
pub const IPC_RMID: i32 = 0;
/// set options
pub const IPC_SET: i32 = 1;
/// get options
pub const IPC_STAT: i32 = 2;

/// For Linux compatibility.
///
/// get info
pub const IPC_INFO: i32 = 3;
