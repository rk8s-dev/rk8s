// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/sem.h`

use crate::{ipc_perm_t, time_t, IPC_R, IPC_W};

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct semid_ds_t {
    /// operation permission struct
    pub sem_perm: ipc_perm_t,
    /// pointer to first semaphore in set
    //__sem_base: *mut sem_t,
    __sem_base: usize,
    /// number of sems in set
    pub sem_nsems: u16,
    /// last operation time
    pub sem_otime: time_t,
    /// last change time
    ///
    /// Times measured in secs since 00:00:00 UTC, Jan. 1, 1970, without leap seconds
    pub sem_ctime: time_t,
}

/// semop's sops parameter structure
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct sembuf_t {
    /// semaphore #
    pub sem_num: u16,
    /// semaphore operation
    pub sem_op: i16,
    /// operation flags
    pub sem_flg: i16,
}

pub const SEM_UNDO: i32 = 0o10_000;

/// commands for semctl
///
/// Return the value of semncnt {READ}
pub const GETNCNT: i32 = 3;
/// Return the value of sempid {READ}
pub const GETPID: i32 = 4;
/// Return the value of semval {READ}
pub const GETVAL: i32 = 5;
/// Return semvals into arg.array {READ}
pub const GETALL: i32 = 6;
/// Return the value of semzcnt {READ}
pub const GETZCNT: i32 = 7;
/// Set the value of semval to arg.val {ALTER}
pub const SETVAL: i32 = 8;
/// Set semvals from arg.array {ALTER}
pub const SETALL: i32 = 9;
/// Like IPC_STAT but treats semid as sema-index
pub const SEM_STAT: i32 = 10;
/// Like IPC_INFO but treats semid as sema-index
pub const SEM_INFO: i32 = 11;

/// Permissions
///
/// alter permission
pub const SEM_A: i32 = IPC_W;
/// read permission
pub const SEM_R: i32 = IPC_R;
