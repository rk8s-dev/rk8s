// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/sem.h`

use crate::{ipc_perm_t, time_t};

/// semop flags
/// undo the operation on exit
pub const SEM_UNDO: i32 = 0x1000;

/// semctl Command Definitions.
/// get sempid
pub const GETPID: i32 = 11;
/// get semval
pub const GETVAL: i32 = 12;
/// get all semval's
pub const GETALL: i32 = 13;
/// get semncnt
pub const GETNCNT: i32 = 14;
/// get semzcnt
pub const GETZCNT: i32 = 15;
/// set semval
pub const SETVAL: i32 = 16;
/// set all semval's
pub const SETALL: i32 = 17;

/// ipcs ctl cmds
pub const SEM_STAT: i32 = 18;
pub const SEM_INFO: i32 = 19;
pub const SEM_STAT_ANY: i32 = 20;

/// Obsolete, used only for backwards compatibility and libc5 compiles
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct semid_ds_t {
    /// permissions .. see ipc.h
    pub sem_perm: ipc_perm_t,

    /// last semop time
    pub sem_otime: time_t,

    /// create/last `semctl()` time
    pub sem_ctime: time_t,

    /// ptr to first semaphore in array
    //pub sem_base: *mut sem_t,
    pub sem_base: usize,

    /// pending operations to be processed
    //pub sem_pending: *mut sem_queue_t,
    pub sem_pending: usize,

    /// last pending operation
    //struct sem_queue **sem_pending_last;
    pub sem_pending_last: usize,

    /// undo requests on this array
    //pub undo: *mut sem_undo,
    pub undo: usize,

    /// no. of semaphores in array
    pub sem_nsems: u16,
}

/// semop system calls takes an array of these.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct sembuf_t {
    /// semaphore index in array
    pub sem_num: u16,
    /// semaphore operation
    pub sem_op: i16,
    /// operation flags
    pub sem_flg: i16,
}

/// arg for semctl system calls.
#[repr(C)]
#[derive(Clone, Copy)]
pub union semun_t {
    /// value for SETVAL
    pub val: i32,
    /// buffer for `IPC_STAT` & `IPC_SET`
    pub buf: *mut semid_ds_t,
    /// array for GETALL & SETALL
    pub array: *mut u16,
    /// buffer for `IPC_INFO`
    pub info_buf: *mut seminfo_t,
    pad: usize,
}

impl Default for semun_t {
    #[inline]
    fn default() -> Self {
        Self { pad: 0 }
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct seminfo_t {
    pub semmap: i32,
    pub semmni: i32,
    pub semmns: i32,
    pub semmnu: i32,
    pub semmsl: i32,
    pub semopm: i32,
    pub semume: i32,
    pub semusz: i32,
    pub semvmx: i32,
    pub semaem: i32,
}

/// SEMMNI, SEMMSL and SEMMNS are default values which can be
/// modified by sysctl.
/// The values has been chosen to be larger than necessary for any
/// known configuration.
///
/// SEMOPM should not be increased beyond 1000, otherwise there is the
/// risk that `semop()/semtimedop()` fails due to kernel memory fragmentation
/// when allocating the sop array.
/// <= IPCMNI  max # of semaphore identifiers
pub const SEMMNI: i32 = 32000;
/// <= `INT_MAX` max num of semaphores per id
pub const SEMMSL: i32 = 32000;
/// <= `INT_MAX` max # of semaphores in system
pub const SEMMNS: i32 = SEMMNI * SEMMSL;
/// <= 1 000 max num of ops per semop call
pub const SEMOPM: i32 = 500;
/// <= 32767 semaphore maximum value
pub const SEMVMX: i32 = 32767;
/// adjust on exit max value
pub const SEMAEM: i32 = SEMVMX;

/// unused
/// max num of undo entries per process
pub const SEMUME: i32 = SEMOPM;
/// num of undo structures system wide
pub const SEMMNU: i32 = SEMMNS;
/// # of entries in semaphore map
pub const SEMMAP: i32 = SEMMNS;
/// sizeof struct `sem_undo`
pub const SEMUSZ: i32 = 20;
