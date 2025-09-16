// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/shm.h`

use crate::{ipc_perm_sysctl_t, ipc_perm_t, pid_t, size_t, time_t, uintptr_t, IPC_R, IPC_W};

/// Attach read-only (else read-write)
pub const SHM_RDONLY: i32 = 010000;
/// Round attach address to SHMLBA
pub const SHM_RND: i32 = 020000;
/// Attach even if segment removed
pub const _SHM_RMLINGER: i32 = 040000;

/// Segment low boundry address multiple
//pub const SHMLBA: i32 = PAGE_SIZE;

pub type shmatt_t = u32;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct shmid_ds_t {
    /// operation permission structure
    pub shm_perm: ipc_perm_t,

    /// size of segment in bytes
    pub shm_segsz: size_t,

    /// process ID of last shm operation
    pub shm_lpid: pid_t,
    /// process ID of creator
    pub shm_cpid: pid_t,

    /// number of current attaches
    pub shm_nattch: shmatt_t,

    /// time of last shmat()
    pub shm_atime: time_t,
    /// time of last shmdt()
    pub shm_dtime: time_t,
    /// time of last change by shmctl()
    pub shm_ctime: time_t,

    // These members are private and used only in the internal
    // implementation of this interface.
    _shm_internal: uintptr_t,
}

/// Some systems (e.g. HP-UX) take these as the second (cmd) arg to shmctl().
/// Lock segment in memory.
pub const SHM_LOCK: i32 = 3;
/// Unlock a segment locked by SHM_LOCK.
pub const SHM_UNLOCK: i32 = 4;

/// Permission definitions used in shmflag arguments to shmat(2) and shmget(2).
/// Provided for source compatibility only; do not use in new code!
/// S_IRUSR, R for owner
pub const SHM_R: i32 = IPC_R;
/// S_IWUSR, W for owner
pub const SHM_W: i32 = IPC_W;

/// System 5 style catch-all structure for shared memory constants that
/// might be of interest to user programs.  Do we really want/need this?
#[repr(C)]
#[derive(Debug, Clone)]
pub struct shminfo_t {
    /// max shared memory segment size (bytes)
    pub shmmax: u64,

    /// min shared memory segment size (bytes)
    pub shmmin: u32,

    /// max number of shared memory identifiers
    pub shmmni: u32,

    /// max shared memory segments per process
    pub shmseg: u32,

    /// max amount of shared memory (pages)
    pub shmall: u32,
}

/// Warning: 64-bit structure padding is needed here
#[repr(C)]
#[derive(Debug, Clone)]
pub struct shmid_ds_sysctl_t {
    pub shm_perm: ipc_perm_sysctl_t,
    pub shm_segsz: u64,

    pub shm_lpid: pid_t,
    pub shm_cpid: pid_t,

    pub shm_atime: time_t,
    pub shm_dtime: time_t,
    pub shm_ctime: time_t,
    pub shm_nattch: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct shm_sysctl_info_t {
    pub shminfo: shminfo_t,
    pub shmids: [shmid_ds_sysctl_t; 1],
}

pub const SHMSEG_FREE: i32 = 0x0200;
pub const SHMSEG_REMOVED: i32 = 0x0400;
pub const SHMSEG_ALLOCATED: i32 = 0x0800;
pub const SHMSEG_WANTED: i32 = 0x1000;
pub const SHMSEG_RMLINGER: i32 = 0x2000;
pub const SHMSEG_WIRED: i32 = 0x4000;
