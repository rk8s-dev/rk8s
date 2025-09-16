// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/arm/include/asm/ptrace.h`

pub const PTRACE_GETREGS: i32 = 12;
pub const PTRACE_SETREGS: i32 = 13;
pub const PTRACE_GETFPREGS: i32 = 14;
pub const PTRACE_SETFPREGS: i32 = 15;
/// `PTRACE_ATTACH` is 16
/// `PTRACE_DETACH` is 17
pub const PTRACE_GETWMMXREGS: i32 = 18;
pub const PTRACE_SETWMMXREGS: i32 = 19;
/// 20 is unused
pub const PTRACE_OLDSETOPTIONS: i32 = 21;
pub const PTRACE_GET_THREAD_AREA: i32 = 22;
pub const PTRACE_SET_SYSCALL: i32 = 23;
/// `PTRACE_SYSCALL` is 24
pub const PTRACE_GETCRUNCHREGS: i32 = 25;
pub const PTRACE_SETCRUNCHREGS: i32 = 26;
pub const PTRACE_GETVFPREGS: i32 = 27;
pub const PTRACE_SETVFPREGS: i32 = 28;
pub const PTRACE_GETHBPREGS: i32 = 29;
pub const PTRACE_SETHBPREGS: i32 = 30;
pub const PTRACE_GETFDPIC: i32 = 31;

pub const PTRACE_GETFDPIC_EXEC: i32 = 0;
pub const PTRACE_GETFDPIC_INTERP: i32 = 1;

/// PSR bits
/// Note on V7M there is no mode contained in the PSR
pub const USR26_MODE: usize = 0x0000_0000;
pub const FIQ26_MODE: usize = 0x0000_0001;
pub const IRQ26_MODE: usize = 0x0000_0002;
pub const SVC26_MODE: usize = 0x0000_0003;
/// Use 0 here to get code right that creates a userspace
/// or kernel space thread.
pub const USR_MODE: usize = 0x0000_0000;
pub const SVC_MODE: usize = 0x0000_0000;
pub const FIQ_MODE: usize = 0x0000_0011;
pub const IRQ_MODE: usize = 0x0000_0012;
pub const MON_MODE: usize = 0x0000_0016;
pub const ABT_MODE: usize = 0x0000_0017;
pub const HYP_MODE: usize = 0x0000_001a;
pub const UND_MODE: usize = 0x0000_001b;
pub const SYSTEM_MODE: usize = 0x0000_001f;
pub const MODE32_BIT: usize = 0x0000_0010;
pub const MODE_MASK: usize = 0x0000_001f;

/// >= V4T, but not V7M
pub const V4_PSR_T_BIT: usize = 0x0000_0020;
pub const V7M_PSR_T_BIT: usize = 0x0100_0000;
pub const PSR_T_BIT: usize = V7M_PSR_T_BIT;

/// >= V4, but not V7M
pub const PSR_F_BIT: usize = 0x0000_0040;
/// >= V4, but not V7M
pub const PSR_I_BIT: usize = 0x0000_0080;
/// >= V6, but not V7M
pub const PSR_A_BIT: usize = 0x0000_0100;
/// >= V6, but not V7M
pub const PSR_E_BIT: usize = 0x0000_0200;
/// >= V5J, but not V7M
pub const PSR_J_BIT: usize = 0x0100_0000;
/// >= V5E, including V7M
pub const PSR_Q_BIT: usize = 0x0800_0000;
pub const PSR_V_BIT: usize = 0x1000_0000;
pub const PSR_C_BIT: usize = 0x2000_0000;
pub const PSR_Z_BIT: usize = 0x4000_0000;
pub const PSR_N_BIT: usize = 0x8000_0000;

/// Groups of PSR bits
/// Flags
pub const PSR_F: usize = 0xff00_0000;
/// Status
pub const PSR_S: usize = 0x00ff_0000;
/// Extension
pub const PSR_X: usize = 0x0000_ff00;
/// Control
pub const PSR_C: usize = 0x0000_00ff;

/// `ARMv7` groups of PSR bits
/// N, Z, C, V, Q and GE flags
pub const APSR_MASK: usize = 0xf80f_0000;
/// ISA state (J, T) mask
pub const PSR_ISET_MASK: usize = 0x0100_0010;
/// If-Then execution state mask
pub const PSR_IT_MASK: usize = 0x0600_fc00;
/// Endianness state mask
pub const PSR_ENDIAN_MASK: usize = 0x0000_0200;

/// Default endianness state
#[cfg(target_endian = "big")]
pub const PSR_ENDSTATE: i32 = PSR_E_BIT;
#[cfg(not(target_endian = "big"))]
pub const PSR_ENDSTATE: i32 = 0;

/// These are 'magic' values for `PTRACE_PEEKUSR` that return info about where a
/// process is located in memory.
pub const PT_TEXT_ADDR: i32 = 0x10000;
pub const PT_DATA_ADDR: i32 = 0x10004;
pub const PT_TEXT_END_ADDR: i32 = 0x10008;

/// This struct defines the way the registers are stored on the
/// stack during a system call.  Note that sizeof(struct `pt_regs`)
/// has to be a multiple of 8.
#[repr(C)]
pub struct pt_regs_t {
    pub uregs: [usize; 18],
}

impl pt_regs_t {
    #[must_use]
    #[inline]
    pub const fn ARM_cpsr(&self) -> usize {
        self.uregs[16]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_pc(&self) -> usize {
        self.uregs[15]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_lr(&self) -> usize {
        self.uregs[14]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_sp(&self) -> usize {
        self.uregs[13]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_ip(&self) -> usize {
        self.uregs[12]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_fp(&self) -> usize {
        self.uregs[11]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r10(&self) -> usize {
        self.uregs[10]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r9(&self) -> usize {
        self.uregs[9]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r8(&self) -> usize {
        self.uregs[8]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r7(&self) -> usize {
        self.uregs[7]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r6(&self) -> usize {
        self.uregs[6]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r5(&self) -> usize {
        self.uregs[5]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r4(&self) -> usize {
        self.uregs[4]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r3(&self) -> usize {
        self.uregs[3]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r2(&self) -> usize {
        self.uregs[2]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r1(&self) -> usize {
        self.uregs[1]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_r0(&self) -> usize {
        self.uregs[0]
    }

    #[must_use]
    #[inline]
    pub const fn ARM_ORIG_r0(&self) -> usize {
        self.uregs[17]
    }
}

/// The size of the user-visible VFP state as seen by `PTRACE_GET/SETVFPREGS`
/// and core dumps.
pub const ARM_VFPREGS_SIZE: usize =  32 * 8 /*fpregs*/ + 4 /*fpscr*/ ;
