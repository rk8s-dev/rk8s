// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `i386/include/mcontext.h`

use core::fmt;

/// mcontext extensions to handle signal delivery.
pub const _UC_SETSTACK: i32 = 0x00010000;
pub const _UC_CLRSTACK: i32 = 0x00020000;
pub const _UC_VM: i32 = 0x00040000;
pub const _UC_TLSBASE: i32 = 0x00080000;

/// Layout of mcontext_t according to the System V Application Binary Interface,
/// Intel386(tm) Architecture Processor Supplement, Fourth Edition.
///
/// General register state
pub const _NGREG: usize = 19;
pub type __greg_t = i32;
pub type __gregset_t = [__greg_t; _NGREG];

pub const _REG_GS: i32 = 0;
pub const _REG_FS: i32 = 1;
pub const _REG_ES: i32 = 2;
pub const _REG_DS: i32 = 3;
pub const _REG_EDI: i32 = 4;
pub const _REG_ESI: i32 = 5;
pub const _REG_EBP: i32 = 6;
pub const _REG_ESP: i32 = 7;
pub const _REG_EBX: i32 = 8;
pub const _REG_EDX: i32 = 9;
pub const _REG_ECX: i32 = 10;
pub const _REG_EAX: i32 = 11;
pub const _REG_TRAPNO: i32 = 12;
pub const _REG_ERR: i32 = 13;
pub const _REG_EIP: i32 = 14;
pub const _REG_CS: i32 = 15;
pub const _REG_EFL: i32 = 16;
pub const _REG_UESP: i32 = 17;
pub const _REG_SS: i32 = 18;

/// x87 regs in fsave format
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct __fpchip_state_t {
    /// Environment and registers
    pub __fp_state: [i32; 27],
}

/// x87 and xmm regs in fxsave format
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct __fp_xmm_state_t {
    //pub __fp_xmm: [c_char; 512],
    pub __fp_xmm: [isize; 64],
}

impl Default for __fp_xmm_state_t {
    fn default() -> Self {
        Self {
            #[rustfmt::skip]
            __fp_xmm: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union __fp_reg_set_un_t {
    __fpchip_state: __fpchip_state_t,
    __fp_xmm_state: __fp_xmm_state_t,
    __fp_fpregs: [i32; 128],
}

impl Default for __fp_reg_set_un_t {
    fn default() -> Self {
        Self {
            __fpchip_state: __fpchip_state_t::default(),
        }
    }
}

impl fmt::Debug for __fp_reg_set_un_t {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = unsafe { self.__fpchip_state };
        write!(f, "fpchip_state: {state:?}")
    }
}

/// Floating point register state
#[repr(C)]
#[derive(Debug, Clone)]
pub struct __fpregset_t {
    pub __fp_reg_set: __fp_reg_set_un_t,

    // Historic padding
    __fp_pad: [i32; 33],
}
//__CTASSERT(sizeof (__fpregset_t) == 512 + 33 * 4);

impl Default for __fpregset_t {
    fn default() -> Self {
        Self {
            __fp_reg_set: __fp_reg_set_un_t::default(),
            #[rustfmt::skip]
            __fp_pad: [
                0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0,
                0,
            ],
        }
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct mcontext_t {
    pub __gregs: __gregset_t,

    pub __fpregs: __fpregset_t,

    pub _mc_tlsbase: __greg_t,
}

/// FP state is in FXSAVE format in XMM space
pub const _UC_FXSAVE: i32 = 0x20;

/// Padding appended to ucontext_t
pub const _UC_MACHINE_PAD: usize = 4;

pub const _UC_UCONTEXT_ALIGN: usize = !0xf;

pub const __UCONTEXT_SIZE: i32 = 776;
