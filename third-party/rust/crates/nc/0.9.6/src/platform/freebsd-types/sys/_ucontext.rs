// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_ucontext.h`

use crate::{mcontext_t, sigset_t, stack_t};

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ucontext_t {
    /// Keep the order of the first two fields.
    ///
    /// Also, keep them the first two fields in the structure.
    /// This way we can have a union with struct sigcontext and ucontext_t.
    /// This allows us to support them both at the same time.
    ///
    /// note: the union is not defined, though.
    pub uc_sigmask: sigset_t,
    pub uc_mcontext: mcontext_t,

    pub uc_link: *mut ucontext_t,
    pub uc_stack: stack_t,
    pub uc_flags: i32,
    __spare__: [i32; 4],
}

impl Default for ucontext_t {
    fn default() -> Self {
        Self {
            uc_sigmask: sigset_t::default(),
            uc_mcontext: mcontext_t::default(),
            uc_link: 0 as *mut ucontext_t,
            uc_stack: stack_t::default(),
            uc_flags: 0,
            __spare__: [0, 0, 0, 0],
        }
    }
}
