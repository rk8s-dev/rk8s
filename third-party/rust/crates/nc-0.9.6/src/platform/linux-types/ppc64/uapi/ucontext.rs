// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From arch/powerpc/include/uapi/asm/ucontext.h

use super::{sigcontext_t, sigset_t, stack_t};

#[repr(C)]
pub struct ucontext_t {
    pub uc_flags: usize,
    pub uc_link: *mut ucontext_t,
    pub uc_stack: stack_t,

    pub uc_sigmask: sigset_t,
    __unused: [sigset_t; 15],      // Allow for uc_sigmask growth
    pub uc_mcontext: sigcontext_t, // last for extensibility
}
