// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/arm64/include/uapi/asm/ptrace.h`

/// PSR bits
pub const PSR_MODE_EL0T: usize = 0x0000_0000;
pub const PSR_MODE_EL1T: usize = 0x0000_0004;
pub const PSR_MODE_EL1H: usize = 0x0000_0005;
pub const PSR_MODE_EL2T: usize = 0x0000_0008;
pub const PSR_MODE_EL2H: usize = 0x0000_0009;
pub const PSR_MODE_EL3T: usize = 0x0000_000c;
pub const PSR_MODE_EL3H: usize = 0x0000_000d;
pub const PSR_MODE_MASK: usize = 0x0000_000f;

/// `AArch32` CPSR bits
pub const PSR_MODE32_BIT: usize = 0x0000_0010;

/// `AArch64` SPSR bits
pub const PSR_F_BIT: usize = 0x0000_0040;
pub const PSR_I_BIT: usize = 0x0000_0080;
pub const PSR_A_BIT: usize = 0x0000_0100;
pub const PSR_D_BIT: usize = 0x0000_0200;
pub const PSR_BTYPE_MASK: usize = 0x0000_0c00;
pub const PSR_SSBS_BIT: usize = 0x0000_1000;
pub const PSR_PAN_BIT: usize = 0x0040_0000;
pub const PSR_UAO_BIT: usize = 0x0080_0000;
pub const PSR_DIT_BIT: usize = 0x0100_0000;
pub const PSR_TCO_BIT: usize = 0x0200_0000;
pub const PSR_V_BIT: usize = 0x1000_0000;
pub const PSR_C_BIT: usize = 0x2000_0000;
pub const PSR_Z_BIT: usize = 0x4000_0000;
pub const PSR_N_BIT: usize = 0x8000_0000;

pub const PSR_BTYPE_SHIFT: i32 = 10;

/// Groups of PSR bits
/// Flags
pub const PSR_F: usize = 0xff00_0000;
/// Status
pub const PSR_S: usize = 0x00ff_0000;
/// Extension
pub const PSR_X: usize = 0x0000_ff00;
/// Control
pub const PSR_C: usize = 0x0000_00ff;

/// Convenience names for the values of `PSTATE.BTYPE`
pub const PSR_BTYPE_NONE: usize = 0b00 << PSR_BTYPE_SHIFT;
pub const PSR_BTYPE_JC: usize = 0b01 << PSR_BTYPE_SHIFT;
pub const PSR_BTYPE_C: usize = 0b10 << PSR_BTYPE_SHIFT;
pub const PSR_BTYPE_J: usize = 0b11 << PSR_BTYPE_SHIFT;

/// syscall emulation path in ptrace
pub const PTRACE_SYSEMU: i32 = 31;
pub const PTRACE_SYSEMU_SINGLESTEP: i32 = 32;
/// MTE allocation tag access
pub const PTRACE_PEEKMTETAGS: i32 = 33;
pub const PTRACE_POKEMTETAGS: i32 = 34;

/// User structures for general purpose, floating point and debug registers.
#[repr(C)]
#[derive(Debug)]
pub struct user_pt_regs_t {
    pub regs: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub pstate: u64,
}

#[repr(C)]
#[derive(Debug)]
pub struct user_fpsimd_state_t {
    pub vregs: [u128; 32],
    pub fpsr: u32,
    pub fpcr: u32,
    __reserved: [u32; 2],
}

#[repr(C)]
#[derive(Debug)]
pub struct user_hwdebug_regs_t {
    pub addr: u64,
    pub ctrl: u32,
    pub pad: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct user_hwdebug_state_t {
    pub dbg_info: u32,
    pub pad: u32,
    pub dbg_regs: [user_hwdebug_regs_t; 16],
}

/// SVE/FP/SIMD state (`NT_ARM_SVE`)
#[repr(C)]
#[derive(Debug)]
pub struct user_sve_header_t {
    /// total meaningful regset content in bytes
    pub size: u32,

    /// maxmium possible size for this thread
    pub max_size: u32,

    /// current vector length
    pub vl: u16,

    /// maximum possible vector length
    pub max_vl: u16,
    pub flags: u16,
    __reserved: u16,
}

/// Definitions for `user_sve_header.flags`:
pub const SVE_PT_REGS_MASK: usize = 1 << 0;

pub const SVE_PT_REGS_FPSIMD: usize = 0;
pub const SVE_PT_REGS_SVE: usize = SVE_PT_REGS_MASK;

/// Common `SVE_PT_*` flags:
/// These must be kept in sync with prctl interface in <linux/prctl.h>
pub const SVE_PT_VL_INHERIT: usize = (1 << 17) /* PR_SVE_VL_INHERIT */ >> 16;
pub const SVE_PT_VL_ONEXEC: usize = (1 << 18) /* PR_SVE_SET_VL_ONEXEC */ >> 16;

// /*
//  * The remainder of the SVE state follows struct user_sve_header.  The
//  * total size of the SVE state (including header) depends on the
//  * metadata in the header:  SVE_PT_SIZE(vq, flags) gives the total size
//  * of the state in bytes, including the header.
//  *
//  * Refer to <asm/sigcontext.h> for details of how to pass the correct
//  * "vq" argument to these macros.
//  */
//
// // Offset from the start of struct user_sve_header to the register data
// #define SVE_PT_REGS_OFFSET						\
// 	((sizeof(struct user_sve_header) + (__SVE_VQ_BYTES - 1))	\
// 		/ __SVE_VQ_BYTES * __SVE_VQ_BYTES)
//
// /*
//  * The register data content and layout depends on the value of the
//  * flags field.
//  */
//
// /*
//  * (flags & SVE_PT_REGS_MASK) == SVE_PT_REGS_FPSIMD case:
//  *
//  * The payload starts at offset SVE_PT_FPSIMD_OFFSET, and is of type
//  * struct user_fpsimd_state.  Additional data might be appended in the
//  * future: use SVE_PT_FPSIMD_SIZE(vq, flags) to compute the total size.
//  * SVE_PT_FPSIMD_SIZE(vq, flags) will never be less than
//  * sizeof(struct user_fpsimd_state).
//  */
//
// pub const SVE_PT_FPSIMD_OFFSET: i32 = SVE_PT_REGS_OFFSET;
//
// #define SVE_PT_FPSIMD_SIZE(vq, flags)	(sizeof(struct user_fpsimd_state))
//
// /*
//  * (flags & SVE_PT_REGS_MASK) == SVE_PT_REGS_SVE case:
//  *
//  * The payload starts at offset SVE_PT_SVE_OFFSET, and is of size
//  * SVE_PT_SVE_SIZE(vq, flags).
//  *
//  * Additional macros describe the contents and layout of the payload.
//  * For each, SVE_PT_SVE_x_OFFSET(args) is the start offset relative to
//  * the start of struct user_sve_header, and SVE_PT_SVE_x_SIZE(args) is
//  * the size in bytes:
//  *
//  *	x	type				description
//  *	-	----				-----------
//  *	ZREGS		\
//  *	ZREG		|
//  *	PREGS		| refer to <asm/sigcontext.h>
//  *	PREG		|
//  *	FFR		/
//  *
//  *	FPSR	uint32_t			FPSR
//  *	FPCR	uint32_t			FPCR
//  *
//  * Additional data might be appended in the future.
//  *
//  * The Z-, P- and FFR registers are represented in memory in an endianness-
//  * invariant layout which differs from the layout used for the FPSIMD
//  * V-registers on big-endian systems: see sigcontext.h for more explanation.
//  */
//
// #define SVE_PT_SVE_ZREG_SIZE(vq)	__SVE_ZREG_SIZE(vq)
// #define SVE_PT_SVE_PREG_SIZE(vq)	__SVE_PREG_SIZE(vq)
// #define SVE_PT_SVE_FFR_SIZE(vq)		__SVE_FFR_SIZE(vq)
// pub const SVE_PT_SVE_FPSR_SIZE: i32 = sizeof;(__u32)
// pub const SVE_PT_SVE_FPCR_SIZE: i32 = sizeof;(__u32)
//
// pub const SVE_PT_SVE_OFFSET: i32 = SVE_PT_REGS_OFFSET;
//
// #define SVE_PT_SVE_ZREGS_OFFSET \
// 	(SVE_PT_REGS_OFFSET + __SVE_ZREGS_OFFSET)
// #define SVE_PT_SVE_ZREG_OFFSET(vq, n) \
// 	(SVE_PT_REGS_OFFSET + __SVE_ZREG_OFFSET(vq, n))
// #define SVE_PT_SVE_ZREGS_SIZE(vq) \
// 	(SVE_PT_SVE_ZREG_OFFSET(vq, __SVE_NUM_ZREGS) - SVE_PT_SVE_ZREGS_OFFSET)
//
// #define SVE_PT_SVE_PREGS_OFFSET(vq) \
// 	(SVE_PT_REGS_OFFSET + __SVE_PREGS_OFFSET(vq))
// #define SVE_PT_SVE_PREG_OFFSET(vq, n) \
// 	(SVE_PT_REGS_OFFSET + __SVE_PREG_OFFSET(vq, n))
// #define SVE_PT_SVE_PREGS_SIZE(vq) \
// 	(SVE_PT_SVE_PREG_OFFSET(vq, __SVE_NUM_PREGS) - \
// 		SVE_PT_SVE_PREGS_OFFSET(vq))
//
// #define SVE_PT_SVE_FFR_OFFSET(vq) \
// 	(SVE_PT_REGS_OFFSET + __SVE_FFR_OFFSET(vq))
//
// #define SVE_PT_SVE_FPSR_OFFSET(vq)				\
// 	((SVE_PT_SVE_FFR_OFFSET(vq) + SVE_PT_SVE_FFR_SIZE(vq) +	\
// 			(__SVE_VQ_BYTES - 1))			\
// 		/ __SVE_VQ_BYTES * __SVE_VQ_BYTES)
// #define SVE_PT_SVE_FPCR_OFFSET(vq) \
// 	(SVE_PT_SVE_FPSR_OFFSET(vq) + SVE_PT_SVE_FPSR_SIZE)
//
// /*
//  * Any future extension appended after FPCR must be aligned to the next
//  * 128-bit boundary.
//  */
//
// #define SVE_PT_SVE_SIZE(vq, flags)					\
// 	((SVE_PT_SVE_FPCR_OFFSET(vq) + SVE_PT_SVE_FPCR_SIZE		\
// 			- SVE_PT_SVE_OFFSET + (__SVE_VQ_BYTES - 1))	\
// 		/ __SVE_VQ_BYTES * __SVE_VQ_BYTES)
//
// #define SVE_PT_SIZE(vq, flags)						\
// 	 (((flags) & SVE_PT_REGS_MASK) == SVE_PT_REGS_SVE ?		\
// 		  SVE_PT_SVE_OFFSET + SVE_PT_SVE_SIZE(vq, flags)	\
// 		: SVE_PT_FPSIMD_OFFSET + SVE_PT_FPSIMD_SIZE(vq, flags))

/// pointer authentication masks (`NT_ARM_PAC_MASK`)
#[repr(C)]
#[derive(Debug)]
pub struct user_pac_mask_t {
    pub data_mask: u64,
    pub insn_mask: u64,
}

/// pointer authentication keys (`NT_ARM_PACA_KEYS`, `NT_ARM_PACG_KEYS`)
#[repr(C)]
#[derive(Debug)]
struct user_pac_address_keys {
    pub apiakey: u128,
    pub apibkey: u128,
    pub apdakey: u128,
    pub apdbkey: u128,
}

#[repr(C)]
#[derive(Debug)]
struct user_pac_generic_keys {
    pub apgakey: u128,
}
