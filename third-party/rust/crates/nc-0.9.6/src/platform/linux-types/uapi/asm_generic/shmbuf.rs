// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/shmbuf.h`

use crate::{ipc64_perm_t, pid_t, size_t};

#[cfg(target_pointer_width = "64")]
use crate::time_t;

/// The `shmid64_ds` structure for x86 architecture.
/// Note extra padding because this structure is passed back and forth
/// between kernel and user space.
///
/// `shmid64_ds` was originally meant to be architecture specific, but
/// everyone just ended up making identical copies without specific
/// optimizations, so we may just as well all use the same one.
///
/// 64 bit architectures typically define a 64 bit `__kernel_time_t`,
/// so they do not need the first two padding words.
/// On big-endian systems, the padding is in the wrong place.  
///
/// Pad space is left for:
/// - 2 miscellaneous 32-bit values
#[cfg(target_pointer_width = "64")]
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct shmid64_ds_t {
    /// operation perms
    pub shm_perm: ipc64_perm_t,
    /// size of segment (bytes)
    pub shm_segsz: size_t,
    /// last attach time
    pub shm_atime: time_t,
    /// last detach time
    pub shm_dtime: time_t,
    /// last change time
    pub shm_ctime: time_t,
    /// pid of creator
    pub shm_cpid: pid_t,
    /// pid of last operator
    pub shm_lpid: pid_t,
    /// no. of current attaches
    pub shm_nattch: usize,
    unused4: usize,
    unused5: usize,
}

#[cfg(target_pointer_width = "32")]
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct shmid64_ds_t {
    /// operation perms
    pub shm_perm: ipc64_perm_t,
    /// size of segment (bytes)
    pub shm_segsz: size_t,
    /// last attach time
    pub shm_atime: usize,
    pub shm_atime_high: usize,
    /// last detach time
    pub shm_dtime: usize,
    pub shm_dtime_high: usize,
    /// last change time
    pub shm_ctime: usize,
    pub shm_ctime_high: usize,
    /// pid of creator
    pub shm_cpid: pid_t,
    /// pid of last operator
    pub shm_lpid: pid_t,
    /// no. of current attaches
    pub shm_nattch: usize,
    unused4: usize,
    unused5: usize,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct shminfo64_t {
    pub shmmax: usize,
    pub shmmin: usize,
    pub shmmni: usize,
    pub shmseg: usize,
    pub shmall: usize,
    unused1: usize,
    unused2: usize,
    unused3: usize,
    unused4: usize,
}
