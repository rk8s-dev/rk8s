// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/posix_types.h`
//!
//! This file is generally used by user-level software, so you need to
//! be a little careful about namespace pollution etc.
//!
//! First the types that are often defined in different ways across
//! architectures, so that you can override them.

pub type __kernel_long_t = isize;
pub type __kernel_ulong_t = usize;

pub type __kernel_ino_t = __kernel_ulong_t;

#[cfg(not(target_arch = "arm"))]
pub type __kernel_mode_t = u32;

pub type __kernel_pid_t = i32;

#[cfg(not(target_arch = "arm"))]
pub type __kernel_ipc_pid_t = i32;

#[cfg(not(target_arch = "arm"))]
pub type __kernel_uid_t = u32;
#[cfg(not(target_arch = "arm"))]
pub type __kernel_gid_t = u32;
pub type __kernel_suseconds_t = __kernel_long_t;

pub type __kernel_daddr_t = i32;

pub type __kernel_uid32_t = u32;
pub type __kernel_gid32_t = u32;

//#[cfg(not(target_arch = "aarch64"))]
//pub type __kernel_old_uid_t = __kernel_uid_t;
//#[cfg(not(target_arch = "aarch64"))]
//pub type __kernel_old_gid_t = __kernel_gid_t;
//#[cfg(not(target_arch = "arm"))]
//pub type __kernel_old_dev_t = u32;

/// Most 32 bit architectures use `unsigned int` `size_t`,
/// and all 64 bit architectures use `unsigned long` `size_t`.
#[cfg(target_pointer_width = "64")]
pub type __kernel_size_t = u32;
#[cfg(target_pointer_width = "64")]
pub type __kernel_ssize_t = i32;
#[cfg(target_pointer_width = "64")]
pub type __kernel_ptrdiff_t = i32;

#[cfg(target_pointer_width = "32")]
pub type __kernel_size_t = __kernel_ulong_t;
#[cfg(target_pointer_width = "32")]
pub type __kernel_ssize_t = __kernel_ulong_t;
#[cfg(target_pointer_width = "32")]
pub type __kernel_ptrdiff_t = __kernel_ulong_t;

#[repr(C)]
#[derive(Debug)]
pub struct __kernel_fsid_t {
    pub val: [i32; 2],
}

/// anything below here should be completely generic
pub type __kernel_off_t = __kernel_long_t;
pub type __kernel_loff_t = i64;
pub type __kernel_old_time_t = __kernel_long_t;
pub type __kernel_time_t = __kernel_long_t;
pub type __kernel_time64_t = i64;

pub type __kernel_clock_t = __kernel_long_t;
pub type __kernel_timer_t = i32;

pub type __kernel_clockid_t = i32;
//typedef char *		__kernel_caddr_t;
pub type __kernel_uid16_t = u16;
pub type __kernel_gid16_t = u16;
