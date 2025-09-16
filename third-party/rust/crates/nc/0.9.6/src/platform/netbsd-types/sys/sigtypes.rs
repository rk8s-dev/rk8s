// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/sigtypes.h`

use crate::size_t;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sigset_t {
    pub __bits: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct stack_t {
    /// signal stack base
    pub ss_sp: usize,

    /// signal stack length
    pub ss_size: size_t,

    /// SS_DISABLE and/or SS_ONSTACK
    pub ss_flags: i32,
}

pub type sigaltstack_t = stack_t;
