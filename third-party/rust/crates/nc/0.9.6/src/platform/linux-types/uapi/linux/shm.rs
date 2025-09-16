// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/shm.h`

#![allow(clippy::module_name_repetitions)]

use crate::{
    ipc_perm_t, ipc_pid_t, time_t, HUGETLB_FLAG_ENCODE_16GB, HUGETLB_FLAG_ENCODE_16MB,
    HUGETLB_FLAG_ENCODE_1GB, HUGETLB_FLAG_ENCODE_1MB, HUGETLB_FLAG_ENCODE_256MB,
    HUGETLB_FLAG_ENCODE_2GB, HUGETLB_FLAG_ENCODE_2MB, HUGETLB_FLAG_ENCODE_32MB,
    HUGETLB_FLAG_ENCODE_512KB, HUGETLB_FLAG_ENCODE_512MB, HUGETLB_FLAG_ENCODE_64KB,
    HUGETLB_FLAG_ENCODE_8MB, HUGETLB_FLAG_ENCODE_MASK, HUGETLB_FLAG_ENCODE_SHIFT,
};

/// SHMMNI, SHMMAX and SHMALL are default upper limits which can be
/// modified by sysctl.
///
/// The SHMMAX and SHMALL values have been chosen to be as large possible
/// without facilitating scenarios where userspace causes overflows
/// when adjusting the limits via operations of the form "retrieve current limit;
/// add X; update limit".
///
/// It is therefore not advised to make SHMMAX and SHMALL any larger.
/// These limits are suitable for both 32 and 64-bit systems.
/// min shared seg size (bytes)
pub const SHMMIN: i32 = 1;
/// max num of segs system wide
pub const SHMMNI: i32 = 4096;
/// max shared seg size (bytes)
pub const SHMMAX: usize = usize::MAX - (1_usize << 24);
/// max shm system wide (pages)
pub const SHMALL: usize = usize::MAX - (1_usize << 24);
/// max shared segs per process
pub const SHMSEG: i32 = SHMMNI;

/// Obsolete, used only for backwards compatibility and libc5 compiles
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct shmid_ds_t {
    /// operation perms
    pub shm_perm: ipc_perm_t,
    /// size of segment (bytes)
    pub shm_segsz: i32,
    /// last attach time
    pub shm_atime: time_t,
    /// last detach time
    pub shm_dtime: time_t,
    ///* last change time
    pub shm_ctime: time_t,
    /// pid of creator
    pub shm_cpid: ipc_pid_t,
    /// pid of last operator
    pub shm_lpid: ipc_pid_t,
    /// no. of current attaches
    pub shm_nattch: u16,
    /// compatibility
    shm_unused: u16,
    /// ditto - used by DIPC
    shm_unused2: usize,
    shm_unused3: usize,
}

/// `shmget()` shmflg values.
/// The bottom nine bits are the same as `open(2)` mode flags
/// or `S_IRUGO` from `<linux/stat.h>`
pub const SHM_R: i32 = 0o400;
/// or `S_IWUGO` from `<linux/stat.h>`
pub const SHM_W: i32 = 0o200;
/// Bits 9 & 10 are `IPC_CREAT` and `IPC_EXCL`
/// segment will use huge TLB pages
pub const SHM_HUGETLB: i32 = 0o4000;
/// don't check for reservations
pub const SHM_NORESERVE: i32 = 0o10_000;

/// Huge page size encoding when `SHM_HUGETLB` is specified, and a huge page
/// size other than the default is desired.  See `hugetlb_encode.h`
pub const SHM_HUGE_SHIFT: i32 = HUGETLB_FLAG_ENCODE_SHIFT;
pub const SHM_HUGE_MASK: i32 = HUGETLB_FLAG_ENCODE_MASK;

pub const SHM_HUGE_64KB: usize = HUGETLB_FLAG_ENCODE_64KB;
pub const SHM_HUGE_512KB: usize = HUGETLB_FLAG_ENCODE_512KB;
pub const SHM_HUGE_1MB: usize = HUGETLB_FLAG_ENCODE_1MB;
pub const SHM_HUGE_2MB: usize = HUGETLB_FLAG_ENCODE_2MB;
pub const SHM_HUGE_8MB: usize = HUGETLB_FLAG_ENCODE_8MB;
pub const SHM_HUGE_16MB: usize = HUGETLB_FLAG_ENCODE_16MB;
pub const SHM_HUGE_32MB: usize = HUGETLB_FLAG_ENCODE_32MB;
pub const SHM_HUGE_256MB: usize = HUGETLB_FLAG_ENCODE_256MB;
pub const SHM_HUGE_512MB: usize = HUGETLB_FLAG_ENCODE_512MB;
pub const SHM_HUGE_1GB: usize = HUGETLB_FLAG_ENCODE_1GB;
pub const SHM_HUGE_2GB: usize = HUGETLB_FLAG_ENCODE_2GB;
pub const SHM_HUGE_16GB: usize = HUGETLB_FLAG_ENCODE_16GB;

/// `shmat()` shmflg values
/// read-only access
pub const SHM_RDONLY: i32 = 0o10_000;
/// round attach address to SHMLBA boundary
pub const SHM_RND: i32 = 0o20_000;
/// take-over region on attach
pub const SHM_REMAP: i32 = 0o40_000;
/// execution access
pub const SHM_EXEC: i32 = 0o100_000;

/// super user shmctl commands
pub const SHM_LOCK: i32 = 11;
pub const SHM_UNLOCK: i32 = 12;

/// ipcs ctl commands
pub const SHM_STAT: i32 = 13;
pub const SHM_INFO: i32 = 14;
pub const SHM_STAT_ANY: i32 = 15;

/// Obsolete, used only for backwards compatibility
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct shminfo_t {
    pub shmmax: i32,
    pub shmmin: i32,
    pub shmmni: i32,
    pub shmseg: i32,
    pub shmall: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct shm_info_t {
    pub used_ids: i32,
    /// total allocated shm
    pub shm_tot: usize,
    /// total resident shm
    pub shm_rss: usize,
    /// total swapped shm
    pub shm_swp: usize,
    swap_attempts: usize,
    swap_successes: usize,
}
