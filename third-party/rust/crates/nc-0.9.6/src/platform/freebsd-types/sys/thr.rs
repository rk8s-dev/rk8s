// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/thr.h`

use crate::{rtprio_t, size_t};

/// Create the thread in the suspended state.
pub const THR_SUSPENDED: i32 = 0x0001;
/// Create the system scope thread.
pub const THR_SYSTEM_SCOPE: i32 = 0x0002;

pub type thr_start_func_t = fn(usize);

#[repr(C)]
#[derive(Debug, Clone)]
pub struct thr_param_t {
    /// thread entry function.
    pub start_func: thr_start_func_t,
    /// argument for entry function.
    pub arg: usize,
    /// stack base address.
    pub stack_base: *mut u8,
    /// stack size.
    pub stack_size: size_t,
    /// tls base address.
    tls_base: *mut u8,
    /// tls size.
    pub tls_size: size_t,
    /// address to store new TID.
    pub child_tid: *mut isize,
    /// parent accesses the new TID here.
    pub parent_tid: *mut isize,
    /// thread flags.
    pub flags: i32,
    /// Real-time scheduling priority
    pub rtp: *mut rtprio_t,
    spare: [usize; 3],
}
