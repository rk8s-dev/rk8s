// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/types.h`

/// fs block count
pub type blkcnt_t = i64;

/// fs optimal block size
pub type blksize_t = i32;

/// device number
pub type dev_t = u64;
/// fixed point number
pub type fixpt_t = u32;

/// group id, process id or user id
pub type id_t = u32;
/// inode number
pub type ino_t = u64;
/// IPC key (for Sys V IPC)
pub type key_t = isize;

/// link count
pub type nlink_t = u32;

/// LWP id
pub type lwpid_t = i32;
/// resource limit
pub type rlim_t = u64;
/// segment size
pub type segsz_t = i32;
/// swap offset
pub type swblk_t = i32;

pub type mqd_t = i32;

pub type cpuid_t = usize;

pub type psetid_t = i32;

pub const NBBY: i32 = 8;

pub type pri_t = i32;

// NOTE(Shaohua): Copied from types.h in freebsd
pub type fflags_t = u32;
pub type intptr_t = isize;
pub type intfptr_t = isize;
pub type uintptr_t = usize;
pub type vm_offset_t = usize;
pub type vm_size_t = usize;
