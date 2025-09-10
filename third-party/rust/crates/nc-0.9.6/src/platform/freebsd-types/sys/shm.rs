// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/shm.h`

use crate::{ipc_perm_t, pid_t, size_t, time_t, IPC_R, IPC_W, PAGE_SIZE};

/// Attach read-only (else read-write)
pub const SHM_RDONLY: i32 = 0o10_000;
/// Round attach address to SHMLBA
pub const SHM_RND: i32 = 0o20_000;
/// Unmap before mapping
pub const SHM_REMAP: i32 = 0o30_000;
/// Segment low boundary address multiple
pub const SHMLBA: usize = PAGE_SIZE;

/// "official" access mode definitions; somewhat braindead since you have to specify
/// `(SHM_* >> 3)` for group and `(SHM_* >> 6)` for world permissions.
pub const SHM_R: i32 = IPC_R;
pub const SHM_W: i32 = IPC_W;

/// predefine tbd `*LOCK` shmctl commands
pub const SHM_LOCK: i32 = 11;
pub const SHM_UNLOCK: i32 = 12;

/// ipcs shmctl commands for Linux compatibility
pub const SHM_STAT: i32 = 13;
pub const SHM_INFO: i32 = 14;

pub type shmatt_t = u32;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct shmid_ds_t {
    /// operation permission structure
    pub shm_perm: ipc_perm_t,
    /// size of segment in bytes
    pub shm_segsz: size_t,
    /// process ID of last shared memory op
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
}

/// System 5 style catch-all structure for shared memory constants that
/// might be of interest to user programs.  Do we really want/need this?
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct shminfo_t {
    /// max shared memory segment size (bytes)
    pub shmmax: usize,
    /// max shared memory segment size (bytes)
    pub shmmin: usize,
    /// max number of shared memory identifiers
    pub shmmni: usize,
    /// max shared memory segments per process
    pub shmseg: usize,
    /// max amount of shared memory (pages)
    pub shmall: usize,
}

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct shm_info_t {
    pub used_ids: i32,
    pub shm_tot: usize,
    pub shm_rss: usize,
    pub shm_swp: usize,
    pub swap_attempts: usize,
    pub swap_successes: usize,
}
