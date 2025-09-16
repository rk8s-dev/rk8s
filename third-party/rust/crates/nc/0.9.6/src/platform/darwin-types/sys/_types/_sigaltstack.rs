// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_sigaltstack.h`

use crate::{size_t, uintptr_t};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct sigaltstack_t {
    /// signal stack base
    pub ss_sp: uintptr_t,

    /// signal stack length
    pub ss_size: size_t,

    /// SA_DISABLE and/or SA_ONSTACK
    pub ss_flags: i32,
}

/// signal stack
pub type stack_t = sigaltstack_t;
