// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/sem.h`

use crate::{ipc_perm_sysctl_t, ipc_perm_t, time_t};

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct semid_ds_t {
    /// operation permission structure
    pub sem_perm: ipc_perm_t,
    /// number of semaphores in set
    pub sem_nsems: u16,
    /// last semop() time
    pub sem_otime: time_t,
    /// last time changed by semctl()
    pub sem_ctime: time_t,

    /// These members are private and used only in the internal
    /// implementation of this interface.
    ///
    /// pointer to first semaphore in set
    //_sem_base: *const __sem_t,
    _sem_base: usize,
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

/// undo changes on process exit
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

/// semaphore info struct
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct seminfo_t {
    /// # of entries in semaphore map
    pub semmap: i32,
    /// # of semaphore identifiers
    pub semmni: i32,
    /// # of semaphores in system
    pub semmns: i32,
    /// # of undo structures in system
    pub semmnu: i32,
    /// max # of semaphores per id
    pub semmsl: i32,
    /// max # of operations per semop call
    pub semopm: i32,
    /// max # of undo entries per process
    pub semume: i32,
    /// size in bytes of undo structure
    pub semusz: i32,
    /// semaphore maximum value
    pub semvmx: i32,
    /// adjust on exit max value
    pub semaem: i32,
}

/// Warning: 64-bit structure padding is needed here
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct semid_ds_sysctl_t {
    pub sem_perm: ipc_perm_sysctl_t,
    pub sem_nsems: i16,
    pub pad2: i16,
    pub pad3: i32,
    pub sem_otime: time_t,
    pub sem_ctime: time_t,
}

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct sem_sysctl_info_t {
    pub seminfo: seminfo_t,
    pub semids: [semid_ds_sysctl_t; 1],
}

/// Internal "mode" bits.  The first of these is used by ipcs(1), and so
/// is defined outside the kernel as well.
///
/// semaphore is allocated
pub const SEM_ALLOC: i32 = 0o1000;
