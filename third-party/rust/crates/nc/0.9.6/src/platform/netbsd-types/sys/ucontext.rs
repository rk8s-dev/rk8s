// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/ucontext.h`

use crate::{mcontext_t, sigset_t, stack_t, _UC_MACHINE_PAD};

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ucontext_t {
    /// properties
    pub uc_flags: u32,

    /// context to resume
    pub uc_link: *mut ucontext_t,

    /// signals blocked in this context
    pub uc_sigmask: sigset_t,

    /// the stack used by this context
    pub uc_stack: stack_t,

    /// machine state
    pub uc_mcontext: mcontext_t,

    __uc_pad: [isize; _UC_MACHINE_PAD],
}

pub const _UC_UCONTEXT_ALIGN: usize = !0;

/// uc_flags
/// valid uc_sigmask
pub const _UC_SIGMASK: i32 = 0x01;
/// valid uc_stack
pub const _UC_STACK: i32 = 0x02;
/// valid GPR context in uc_mcontext
pub const _UC_CPU: i32 = 0x04;
/// valid FPU context in uc_mcontext
pub const _UC_FPU: i32 = 0x08;
/// MD bits.  see below
pub const _UC_MD: i32 = 0x400f0020;
