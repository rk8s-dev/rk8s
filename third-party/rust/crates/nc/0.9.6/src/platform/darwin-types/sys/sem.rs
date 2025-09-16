// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/sem.h`

use crate::{ipc_perm_t, pid_t, time_t};

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct semid_ds {
    /// operation permission struct
    pub sem_perm: ipc_perm_t,
    /// 32 bit base ptr for semaphore set
    pub sem_base: i32,
    /// number of sems in set
    pub sem_nsems: u16,
    /// last operation time
    pub sem_otime: time_t,
    // RESERVED: DO NOT USE!
    sem_pad1: i32,
    /// last change time
    ///
    /// Times measured in secs since 00:00:00 GMT, Jan. 1, 1970
    pub sem_ctime: time_t,
    // RESERVED: DO NOT USE!
    sem_pad2: i32,
    // RESERVED: DO NOT USE!
    sem_pad3: [i32; 4],
}

/// Possible values for the third argument to semctl()
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

/// A semaphore; this is an anonymous structure, not for external use
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct sem_t {
    /// semaphore value
    pub semval: u16,
    /// pid of last operation
    pub sempid: pid_t,
    /// # awaiting semval > cval
    pub semncnt: u16,
    /// # awaiting semval == 0
    pub semzcnt: u16,
}

/// Structure of array element for second argument to semop()
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

/// Possible flag values for sem_flg
///
/// Set up adjust on exit entry
pub const SEM_UNDO: i32 = 0o10000;

//#[repr(C)]
//pub union semun_u {
//    /// value for SETVAL
//    val: i32,
//    /// buffer for IPC_STAT & IPC_SET
//    buf: *mut semid_ds_t,
//    /// array for GETALL & SETALL
//    array: *mut u16,
//}

/// Permissions
///
/// alter permission
pub const SEM_A: i32 = 0o200;
/// read permission
pub const SEM_R: i32 = 0o400;
