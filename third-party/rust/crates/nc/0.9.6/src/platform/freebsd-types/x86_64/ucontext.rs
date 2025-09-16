// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `amd64/ucontext.h`

use crate::register_t;

/// mc_flags bits. Shall be in sync with TF_XXX.
pub const _MC_HASSEGS: i32 = 0x1;
pub const _MC_HASBASES: i32 = 0x2;
pub const _MC_HASFPXSTATE: i32 = 0x4;
pub const _MC_FLAG_MASK: i32 = _MC_HASSEGS | _MC_HASBASES | _MC_HASFPXSTATE;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct mcontext_t {
    /// The definition of mcontext_t must match the layout of struct sigcontext
    /// after the sc_mask member.
    ///
    /// This is so that we can support sigcontext and ucontext_t at the same time.
    // XXX - sigcontext compat.
    pub mc_onstack: register_t,
    // machine state (struct trapframe)
    pub mc_rdi: register_t,
    pub mc_rsi: register_t,
    pub mc_rdx: register_t,
    pub mc_rcx: register_t,
    pub mc_r8: register_t,
    pub mc_r9: register_t,
    pub mc_rax: register_t,
    pub mc_rbx: register_t,
    pub mc_rbp: register_t,
    pub mc_r10: register_t,
    pub mc_r11: register_t,
    pub mc_r12: register_t,
    pub mc_r13: register_t,
    pub mc_r14: register_t,
    pub mc_r15: register_t,
    pub mc_trapno: u32,
    pub mc_fs: u16,
    pub mc_gs: u16,
    pub mc_addr: register_t,
    pub mc_flags: u32,
    pub mc_es: u16,
    pub mc_ds: u16,
    pub mc_err: register_t,
    pub mc_rip: register_t,
    pub mc_cs: register_t,
    pub mc_rflags: register_t,
    pub mc_rsp: register_t,
    pub mc_ss: register_t,

    /// sizeof(mcontext_t)
    pub mc_len: isize,

    pub mc_fpformat: isize,
    pub mc_ownedfp: isize,
    // See <machine/fpu.h> for the internals of mc_fpstate[].
    // TODO(Shaohua): Add aligned property.
    //long	mc_fpstate [64] __aligned(16);
    pub mc_fpstate: [isize; 64],

    pub mc_fsbase: register_t,
    pub mc_gsbase: register_t,

    pub mc_xfpustate: register_t,
    pub mc_xfpustate_len: register_t,

    mc_spare: [isize; 4],
}

impl Default for mcontext_t {
    fn default() -> Self {
        Self {
            mc_onstack: 0,
            mc_rdi: 0,
            mc_rsi: 0,
            mc_rdx: 0,
            mc_rcx: 0,
            mc_r8: 0,
            mc_r9: 0,
            mc_rax: 0,
            mc_rbx: 0,
            mc_rbp: 0,
            mc_r10: 0,
            mc_r11: 0,
            mc_r12: 0,
            mc_r13: 0,
            mc_r14: 0,
            mc_r15: 0,
            mc_trapno: 0,
            mc_fs: 0,
            mc_gs: 0,
            mc_addr: 0,
            mc_flags: 0,
            mc_es: 0,
            mc_ds: 0,
            mc_err: 0,
            mc_rip: 0,
            mc_cs: 0,
            mc_rflags: 0,
            mc_rsp: 0,
            mc_ss: 0,

            mc_len: 0,

            mc_fpformat: 0,
            mc_ownedfp: 0,
            #[rustfmt::skip]
            mc_fpstate: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],

            mc_fsbase: 0,
            mc_gsbase: 0,

            mc_xfpustate: 0,
            mc_xfpustate_len: 0,

            mc_spare: [0, 0, 0, 0],
        }
    }
}

/// device not present or configured
pub const _MC_FPFMT_NODEV: isize = 0x10000;
pub const _MC_FPFMT_XMM: isize = 0x10002;

/// FP state not used
pub const _MC_FPOWNED_NONE: isize = 0x20000;
/// FP state came from FPU
pub const _MC_FPOWNED_FPU: isize = 0x20001;
/// FP state came from PCB
pub const _MC_FPOWNED_PCB: isize = 0x20002;
