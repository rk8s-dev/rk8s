// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/powerpc/include/uapi/asm/ptrace.h`

#[repr(C)]
#[derive(Debug, Clone)]
pub struct user_pt_regs_t {
    pub gpr: [usize; 32],
    pub nip: usize,
    pub msr: usize,
    // Used for restarting system calls
    pub orig_gpr3: usize,
    pub ctr: usize,
    pub link: usize,
    pub xer: usize,
    pub ccr: usize,
    // Soft enabled/disabled
    pub softe: usize,
    // Reason for being here N.B.
    // for critical exceptions on 4xx, the dar and
    // dsisr fields are overloaded to hold srr0 and srr1.
    pub trap: usize,
    // Fault registers
    pub dar: usize,
    // on 4xx/Book-E used for ESR
    pub dsisr: usize,
    // Result of a system call
    pub result: usize,
}

/// Offsets used by 'ptrace' system call interface.
///
/// These can't be changed without breaking binary compatibility
/// with MkLinux, etc.
pub const PT_R0: usize = 0;
pub const PT_R1: usize = 1;
pub const PT_R2: usize = 2;
pub const PT_R3: usize = 3;
pub const PT_R4: usize = 4;
pub const PT_R5: usize = 5;
pub const PT_R6: usize = 6;
pub const PT_R7: usize = 7;
pub const PT_R8: usize = 8;
pub const PT_R9: usize = 9;
pub const PT_R10: usize = 10;
pub const PT_R11: usize = 11;
pub const PT_R12: usize = 12;
pub const PT_R13: usize = 13;
pub const PT_R14: usize = 14;
pub const PT_R15: usize = 15;
pub const PT_R16: usize = 16;
pub const PT_R17: usize = 17;
pub const PT_R18: usize = 18;
pub const PT_R19: usize = 19;
pub const PT_R20: usize = 20;
pub const PT_R21: usize = 21;
pub const PT_R22: usize = 22;
pub const PT_R23: usize = 23;
pub const PT_R24: usize = 24;
pub const PT_R25: usize = 25;
pub const PT_R26: usize = 26;
pub const PT_R27: usize = 27;
pub const PT_R28: usize = 28;
pub const PT_R29: usize = 29;
pub const PT_R30: usize = 30;
pub const PT_R31: usize = 31;

pub const PT_NIP: usize = 32;
pub const PT_MSR: usize = 33;
pub const PT_ORIG_R3: usize = 34;
pub const PT_CTR: usize = 35;
pub const PT_LNK: usize = 36;
pub const PT_XER: usize = 37;
pub const PT_CCR: usize = 38;
pub const PT_SOFTE: usize = 39;
pub const PT_TRAP: usize = 40;
pub const PT_DAR: usize = 41;
pub const PT_DSISR: usize = 42;
pub const PT_RESULT: usize = 43;
pub const PT_DSCR: usize = 44;
pub const PT_REGS_COUNT: usize = 44;

/// each FP reg occupies 2 slots in this space
pub const PT_FPR0: usize = 48;

// each FP reg occupies 1 slot in 64-bit space
pub const PT_FPSCR: usize = PT_FPR0 + 32;

/// each Vector reg occupies 2 slots in 64-bit
pub const PT_VR0: usize = 82;
pub const PT_VSCR: usize = PT_VR0 + 32 * 2 + 1;
pub const PT_VRSAVE: usize = PT_VR0 + 33 * 2;

// Only store first 32 VSRs here. The second 32 VSRs in VR0-31
// each VSR reg occupies 2 slots in 64-bit
pub const PT_VSR0: usize = 150;
pub const PT_VSR31: usize = PT_VSR0 + 2 * 31;

/*
 * Get/set all the altivec registers v0..v31, vscr, vrsave, in one go.
 * The transfer totals 34 quadword.  Quadwords 0-31 contain the
 * corresponding vector registers.  Quadword 32 contains the vscr as the
 * last word (offset 12) within that quadword.  Quadword 33 contains the
 * vrsave as the first word (offset 0) within the quadword.
 *
 * This definition of the VMX state is compatible with the current PPC32
 * ptrace interface.  This allows signal handling and ptrace to use the same
 * structures.  This also simplifies the implementation of a bi-arch
 * (combined (32- and 64-bit) gdb.
 */
pub const PTRACE_GETVRREGS: i32 = 0x12;
pub const PTRACE_SETVRREGS: i32 = 0x13;

/// Get/set all the upper 32-bits of the SPE registers, accumulator, and spefscr, in one go.
pub const PTRACE_GETEVRREGS: i32 = 0x14;
pub const PTRACE_SETEVRREGS: i32 = 0x15;

/// Get the first 32 128bit VSX registers
pub const PTRACE_GETVSRREGS: i32 = 0x1b;
pub const PTRACE_SETVSRREGS: i32 = 0x1c;

/// Syscall emulation defines
pub const PTRACE_SYSEMU: i32 = 0x1d;
pub const PTRACE_SYSEMU_SINGLESTEP: i32 = 0x1e;

/// Get or set a debug register. The first 16 are DABR registers and
/// the second 16 are IABR registers.
pub const PTRACE_GET_DEBUGREG: i32 = 0x19;
pub const PTRACE_SET_DEBUGREG: i32 = 0x1a;

/// (new) PTRACE requests using the same numbers as x86 and the same
/// argument ordering.
///
/// Additionally, they support more registers too
pub const PTRACE_GETREGS: i32 = 0xc;
pub const PTRACE_SETREGS: i32 = 0xd;
pub const PTRACE_GETFPREGS: i32 = 0xe;
pub const PTRACE_SETFPREGS: i32 = 0xf;
pub const PTRACE_GETREGS64: i32 = 0x16;
pub const PTRACE_SETREGS64: i32 = 0x17;

/// Calls to trace a 64bit program from a 32bit program
pub const PPC_PTRACE_PEEKTEXT_3264: i32 = 0x95;
pub const PPC_PTRACE_PEEKDATA_3264: i32 = 0x94;
pub const PPC_PTRACE_POKETEXT_3264: i32 = 0x93;
pub const PPC_PTRACE_POKEDATA_3264: i32 = 0x92;
pub const PPC_PTRACE_PEEKUSR_3264: i32 = 0x91;
pub const PPC_PTRACE_POKEUSR_3264: i32 = 0x90;

/// resume execution until next branch
pub const PTRACE_SINGLEBLOCK: i32 = 0x100;

pub const PPC_PTRACE_GETHWDBGINFO: i32 = 0x89;
pub const PPC_PTRACE_SETHWDEBUG: i32 = 0x88;
pub const PPC_PTRACE_DELHWDEBUG: i32 = 0x87;

/// features will have bits indication whether there is support for:
pub const PPC_DEBUG_FEATURE_INSN_BP_RANGE: u64 = 0x0000000000000001;
pub const PPC_DEBUG_FEATURE_INSN_BP_MASK: u64 = 0x0000000000000002;
pub const PPC_DEBUG_FEATURE_DATA_BP_RANGE: u64 = 0x0000000000000004;
pub const PPC_DEBUG_FEATURE_DATA_BP_MASK: u64 = 0x0000000000000008;
pub const PPC_DEBUG_FEATURE_DATA_BP_DAWR: u64 = 0x0000000000000010;
pub const PPC_DEBUG_FEATURE_DATA_BP_ARCH_31: u64 = 0x0000000000000020;

#[repr(C)]
pub struct ppc_hw_breakpoint_t {
    /// currently, version must be 1
    pub version: u32,

    /// only some combinations allowed
    pub trigger_type: u32,

    /// address match mode
    pub addr_mode: u32,

    /// break/watchpoint condition flags
    pub condition_mode: u32,

    /// break/watchpoint address
    pub addr: u64,

    /// range end or mask
    pub addr2: u64,

    /// contents of the DVC register
    pub condition_value: u64,
}

/// Trigger Type
pub const PPC_BREAKPOINT_TRIGGER_EXECUTE: i32 = 0x00000001;
pub const PPC_BREAKPOINT_TRIGGER_READ: i32 = 0x00000002;
pub const PPC_BREAKPOINT_TRIGGER_WRITE: i32 = 0x00000004;
pub const PPC_BREAKPOINT_TRIGGER_RW: i32 =
    PPC_BREAKPOINT_TRIGGER_READ | PPC_BREAKPOINT_TRIGGER_WRITE;

/// Address Mode
pub const PPC_BREAKPOINT_MODE_EXACT: i32 = 0x00000000;
pub const PPC_BREAKPOINT_MODE_RANGE_INCLUSIVE: i32 = 0x00000001;
pub const PPC_BREAKPOINT_MODE_RANGE_EXCLUSIVE: i32 = 0x00000002;
pub const PPC_BREAKPOINT_MODE_MASK: i32 = 0x00000003;

/// Condition Mode
pub const PPC_BREAKPOINT_CONDITION_MODE: i32 = 0x00000003;
pub const PPC_BREAKPOINT_CONDITION_NONE: i32 = 0x00000000;
pub const PPC_BREAKPOINT_CONDITION_AND: i32 = 0x00000001;
pub const PPC_BREAKPOINT_CONDITION_EXACT: i32 = PPC_BREAKPOINT_CONDITION_AND;
pub const PPC_BREAKPOINT_CONDITION_OR: i32 = 0x00000002;
pub const PPC_BREAKPOINT_CONDITION_AND_OR: i32 = 0x00000003;
pub const PPC_BREAKPOINT_CONDITION_BE_ALL: i32 = 0x00ff0000;
pub const PPC_BREAKPOINT_CONDITION_BE_SHIFT: i32 = 16;
