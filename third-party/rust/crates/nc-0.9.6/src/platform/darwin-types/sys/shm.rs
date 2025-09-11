// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/shm.h`

use crate::{ipc_perm_t, pid_t, size_t, time_t, IPC_R, IPC_W};

/// The unsigned integer type used for the number of current attaches
/// that MUST be able to store values at least as large as a type unsigned short.
pub type shmatt_t = u16;

/// Possible flag values which may be OR'ed into the third argument to shmat()
///
/// Attach read-only (else read-write)
pub const SHM_RDONLY: i32 = 0o10000;
/// Round attach address to SHMLBA
pub const SHM_RND: i32 = 0o20000;

/// This value is symbolic, and generally not expected to be sed by user
/// programs directly, although such ise is permitted by the standard.  Its
/// value in our implementation is equal to the number of bytes per page.
///
/// Segment low boundary address multiple
#[cfg(target_arch = "aarch64")]
pub const SHMLBA: usize = 16 * 1024;

/// Segment low boundary address multiple
#[cfg(not(target_arch = "aarch64"))]
pub const SHMLBA: i32 = 4096;

/// "official" access mode definitions; somewhat braindead since you have
/// to specify `(SHM_* >> 3)` for group and `(SHM_* >> 6)` for world permissions
pub const SHM_R: i32 = IPC_R;
pub const SHM_W: i32 = IPC_W;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct shmid_ds_t {
    /// Operation permission value
    pub shm_perm: ipc_perm_t,
    /// Size of segment in bytes
    pub shm_segsz: size_t,
    /// PID of last shared memory op
    pub shm_lpid: pid_t,
    /// PID of creator
    pub shm_cpid: pid_t,
    /// Number of current attaches
    pub shm_nattch: shmatt_t,
    /// Time of last shmat()
    pub shm_atime: time_t,
    /// Time of last shmdt()
    pub shm_dtime: time_t,
    /// Time of last shmctl() change
    pub shm_ctime: time_t,
    // reserved for kernel use
    shm_internal: usize,
}
