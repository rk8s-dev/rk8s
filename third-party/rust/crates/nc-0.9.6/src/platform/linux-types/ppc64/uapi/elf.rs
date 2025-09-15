// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/powerpc/include/uapi/asm/elf.h`
//!
//! ELF register definitions..

use crate::{__vector128_t, ELFCLASS64, EM_PPC64};

#[cfg(target_endian = "little")]
use crate::ELFDATA2LSB;
#[cfg(target_endian = "big")]
use crate::ELFDATA2MSB;

/// PowerPC relocations defined by the ABIs
pub const R_PPC_NONE: usize = 0;
/// 32bit absolute address
pub const R_PPC_ADDR32: usize = 1;
/// 26bit address, 2 bits ignored.
pub const R_PPC_ADDR24: usize = 2;
/// 16bit absolute address
pub const R_PPC_ADDR16: usize = 3;
/// lower 16bit of absolute address
pub const R_PPC_ADDR16_LO: usize = 4;
/// high 16bit of absolute address
pub const R_PPC_ADDR16_HI: usize = 5;
/// adjusted high 16bit
pub const R_PPC_ADDR16_HA: usize = 6;
/// 16bit address, 2 bits ignored
pub const R_PPC_ADDR14: usize = 7;
pub const R_PPC_ADDR14_BRTAKEN: usize = 8;
pub const R_PPC_ADDR14_BRNTAKEN: usize = 9;
/// PC relative 26 bit
pub const R_PPC_REL24: usize = 10;
/// PC relative 16 bit
pub const R_PPC_REL14: usize = 11;
pub const R_PPC_REL14_BRTAKEN: usize = 12;
pub const R_PPC_REL14_BRNTAKEN: usize = 13;
pub const R_PPC_GOT16: usize = 14;
pub const R_PPC_GOT16_LO: usize = 15;
pub const R_PPC_GOT16_HI: usize = 16;
pub const R_PPC_GOT16_HA: usize = 17;
pub const R_PPC_PLTREL24: usize = 18;
pub const R_PPC_COPY: usize = 19;
pub const R_PPC_GLOB_DAT: usize = 20;
pub const R_PPC_JMP_SLOT: usize = 21;
pub const R_PPC_RELATIVE: usize = 22;
pub const R_PPC_LOCAL24PC: usize = 23;
pub const R_PPC_UADDR32: usize = 24;
pub const R_PPC_UADDR16: usize = 25;
pub const R_PPC_REL32: usize = 26;
pub const R_PPC_PLT32: usize = 27;
pub const R_PPC_PLTREL32: usize = 28;
pub const R_PPC_PLT16_LO: usize = 29;
pub const R_PPC_PLT16_HI: usize = 30;
pub const R_PPC_PLT16_HA: usize = 31;
pub const R_PPC_SDAREL16: usize = 32;
pub const R_PPC_SECTOFF: usize = 33;
pub const R_PPC_SECTOFF_LO: usize = 34;
pub const R_PPC_SECTOFF_HI: usize = 35;
pub const R_PPC_SECTOFF_HA: usize = 36;

/// PowerPC relocations defined for the TLS access ABI.
/// none	(sym+add)@tls
pub const R_PPC_TLS: usize = 67;
/// word32	(sym+add)@dtpmod
pub const R_PPC_DTPMOD32: usize = 68;
/// half16*	(sym+add)@tprel
pub const R_PPC_TPREL16: usize = 69;
/// half16	(sym+add)@tprel@l
pub const R_PPC_TPREL16_LO: usize = 70;
/// half16	(sym+add)@tprel@h
pub const R_PPC_TPREL16_HI: usize = 71;
/// half16	(sym+add)@tprel@ha
pub const R_PPC_TPREL16_HA: usize = 72;
/// word32	(sym+add)@tprel
pub const R_PPC_TPREL32: usize = 73;
/// half16*	(sym+add)@dtprel
pub const R_PPC_DTPREL16: usize = 74;
/// half16	(sym+add)@dtprel@l
pub const R_PPC_DTPREL16_LO: usize = 75;
/// half16	(sym+add)@dtprel@h
pub const R_PPC_DTPREL16_HI: usize = 76;
/// half16	(sym+add)@dtprel@ha
pub const R_PPC_DTPREL16_HA: usize = 77;
/// word32	(sym+add)@dtprel
pub const R_PPC_DTPREL32: usize = 78;
/// half16*	(sym+add)@got@tlsgd
pub const R_PPC_GOT_TLSGD16: usize = 79;
/// half16	(sym+add)@got@tlsgd@l
pub const R_PPC_GOT_TLSGD16_LO: usize = 80;
/// half16	(sym+add)@got@tlsgd@h
pub const R_PPC_GOT_TLSGD16_HI: usize = 81;
/// half16	(sym+add)@got@tlsgd@ha
pub const R_PPC_GOT_TLSGD16_HA: usize = 82;
/// half16*	(sym+add)@got@tlsld
pub const R_PPC_GOT_TLSLD16: usize = 83;
/// half16	(sym+add)@got@tlsld@l
pub const R_PPC_GOT_TLSLD16_LO: usize = 84;
/// half16	(sym+add)@got@tlsld@h
pub const R_PPC_GOT_TLSLD16_HI: usize = 85;
/// half16	(sym+add)@got@tlsld@ha
pub const R_PPC_GOT_TLSLD16_HA: usize = 86;
/// half16*	(sym+add)@got@tprel
pub const R_PPC_GOT_TPREL16: usize = 87;
/// half16	(sym+add)@got@tprel@l
pub const R_PPC_GOT_TPREL16_LO: usize = 88;
/// half16	(sym+add)@got@tprel@h
pub const R_PPC_GOT_TPREL16_HI: usize = 89;
/// half16	(sym+add)@got@tprel@ha
pub const R_PPC_GOT_TPREL16_HA: usize = 90;
/// half16*	(sym+add)@got@dtprel
pub const R_PPC_GOT_DTPREL16: usize = 91;
/// half16*	(sym+add)@got@dtprel@l
pub const R_PPC_GOT_DTPREL16_LO: usize = 92;
/// half16*	(sym+add)@got@dtprel@h
pub const R_PPC_GOT_DTPREL16_HI: usize = 93;
/// half16*	(sym+add)@got@dtprel@ha
pub const R_PPC_GOT_DTPREL16_HA: usize = 94;

/// keep this the last entry.
pub const R_PPC_NUM: usize = 95;

/// includes nip, msr, lr, etc.
pub const ELF_NGREG: usize = 48;
/// includes fpscr
pub const ELF_NFPREG: usize = 33;
/// includes all vector registers
pub const ELF_NVMX: usize = 34;
/// includes all VSX registers
pub const ELF_NVSX: usize = 32;
/// include tfhar, tfiar, texasr
pub const ELF_NTMSPRREG: usize = 3;
/// includes ebbrr, ebbhr, bescr
pub const ELF_NEBB: usize = 3;
/// includes siar, sdar, sier, mmcr2, mmcr0
pub const ELF_NPMU: usize = 5;
/// includes amr, iamr, uamor
pub const ELF_NPKEY: usize = 3;

pub type elf_greg_t64_t = usize;
pub type elf_gregset_t64_t = [elf_greg_t64_t; ELF_NGREG];

pub type elf_greg_t32_t = u32;
pub type elf_gregset_t32_t = [elf_greg_t32_t; ELF_NGREG];
pub type compat_elf_gregset_t = elf_gregset_t32_t;

/// ELF_ARCH, CLASS, and DATA are used to set parameters in the core dumps.
/// includes vscr & vrsave stuffed together
pub const ELF_NVRREG32: usize = 33;
/// includes vscr & vrsave in split vectors
pub const ELF_NVRREG: usize = 34;
/// Half the vsx registers
pub const ELF_NVSRHALFREG: usize = 32;
pub type ELF_GREG_TYPE = elf_greg_t64_t;
pub const ELF_ARCH: i32 = EM_PPC64;
pub const ELF_CLASS: i32 = ELFCLASS64;
pub type elf_greg_t = elf_greg_t64_t;
pub type elf_gregset_t = elf_gregset_t64_t;

#[cfg(target_endian = "big")]
pub const ELF_DATA: i32 = ELFDATA2MSB;
#[cfg(target_endian = "little")]
pub const ELF_DATA: i32 = ELFDATA2LSB;

/// Floating point registers
pub type elf_fpreg_t = f64;
pub type elf_fpregset_t = [elf_fpreg_t; ELF_NFPREG];

// Altivec registers
/// The entries with indexes 0-31 contain the corresponding vector registers.
/// The entry with index 32 contains the vscr as the last word (offset 12)
/// within the quadword.  This allows the vscr to be stored as either a
/// quadword (since it must be copied via a vector register to/from storage)
/// or as a word.  
///
/// 64-bit kernel notes: The entry at index 33 contains the vrsave as the first  
/// word (offset 0) within the quadword.
///
/// This definition of the VMX state is compatible with the current PPC32
/// ptrace interface.  This allows signal handling and ptrace to use the same
/// structures.  This also simplifies the implementation of a bi-arch
/// (combined (32- and 64-bit) gdb.
///
/// Note that it's _not_ compatible with 32 bits ucontext which stuffs the
/// vrsave along with vscr and so only uses 33 vectors for the register set
pub type elf_vrreg_t = __vector128_t;
pub type elf_vrregset_t = [elf_vrreg_t; ELF_NVRREG];
pub type elf_vrregset_t32_t = [elf_vrreg_t; ELF_NVRREG32];
pub type elf_vsrreghalf_t32_t = [elf_fpreg_t; ELF_NVSRHALFREG];

/// PowerPC64 relocations defined by the ABIs
pub const R_PPC64_NONE: usize = R_PPC_NONE;
/// 32bit absolute address.
pub const R_PPC64_ADDR32: usize = R_PPC_ADDR32;
/// 26bit address, word aligned.
pub const R_PPC64_ADDR24: usize = R_PPC_ADDR24;
/// 16bit absolute address.
pub const R_PPC64_ADDR16: usize = R_PPC_ADDR16;
/// lower 16bits of abs. address.
pub const R_PPC64_ADDR16_LO: usize = R_PPC_ADDR16_LO;
/// high 16bits of abs. address.
pub const R_PPC64_ADDR16_HI: usize = R_PPC_ADDR16_HI;
/// adjusted high 16bits.
pub const R_PPC64_ADDR16_HA: usize = R_PPC_ADDR16_HA;
/// 16bit address, word aligned.
pub const R_PPC64_ADDR14: usize = R_PPC_ADDR14;
pub const R_PPC64_ADDR14_BRTAKEN: usize = R_PPC_ADDR14_BRTAKEN;
pub const R_PPC64_ADDR14_BRNTAKEN: usize = R_PPC_ADDR14_BRNTAKEN;
/// PC relative 26 bit, word aligned.
pub const R_PPC64_REL24: usize = R_PPC_REL24;
/// PC relative 16 bit.
pub const R_PPC64_REL14: usize = R_PPC_REL14;
pub const R_PPC64_REL14_BRTAKEN: usize = R_PPC_REL14_BRTAKEN;
pub const R_PPC64_REL14_BRNTAKEN: usize = R_PPC_REL14_BRNTAKEN;
pub const R_PPC64_GOT16: usize = R_PPC_GOT16;
pub const R_PPC64_GOT16_LO: usize = R_PPC_GOT16_LO;
pub const R_PPC64_GOT16_HI: usize = R_PPC_GOT16_HI;
pub const R_PPC64_GOT16_HA: usize = R_PPC_GOT16_HA;

pub const R_PPC64_COPY: usize = R_PPC_COPY;
pub const R_PPC64_GLOB_DAT: usize = R_PPC_GLOB_DAT;
pub const R_PPC64_JMP_SLOT: usize = R_PPC_JMP_SLOT;
pub const R_PPC64_RELATIVE: usize = R_PPC_RELATIVE;

pub const R_PPC64_UADDR32: usize = R_PPC_UADDR32;
pub const R_PPC64_UADDR16: usize = R_PPC_UADDR16;
pub const R_PPC64_REL32: usize = R_PPC_REL32;
pub const R_PPC64_PLT32: usize = R_PPC_PLT32;
pub const R_PPC64_PLTREL32: usize = R_PPC_PLTREL32;
pub const R_PPC64_PLT16_LO: usize = R_PPC_PLT16_LO;
pub const R_PPC64_PLT16_HI: usize = R_PPC_PLT16_HI;
pub const R_PPC64_PLT16_HA: usize = R_PPC_PLT16_HA;

pub const R_PPC64_SECTOFF: usize = R_PPC_SECTOFF;
pub const R_PPC64_SECTOFF_LO: usize = R_PPC_SECTOFF_LO;
pub const R_PPC64_SECTOFF_HI: usize = R_PPC_SECTOFF_HI;
pub const R_PPC64_SECTOFF_HA: usize = R_PPC_SECTOFF_HA;
/// word30 (S + A - P) >> 2.
pub const R_PPC64_ADDR30: usize = 37;
/// doubleword64 S + A.
pub const R_PPC64_ADDR64: usize = 38;
/// half16 #higher(S + A).
pub const R_PPC64_ADDR16_HIGHER: usize = 39;
/// half16 #highera(S + A).
pub const R_PPC64_ADDR16_HIGHERA: usize = 40;
/// half16 #highest(S + A).
pub const R_PPC64_ADDR16_HIGHEST: usize = 41;
/// half16 #highesta(S + A).
pub const R_PPC64_ADDR16_HIGHESTA: usize = 42;
/// doubleword64 S + A.
pub const R_PPC64_UADDR64: usize = 43;
/// doubleword64 S + A - P.
pub const R_PPC64_REL64: usize = 44;
/// doubleword64 L + A.
pub const R_PPC64_PLT64: usize = 45;
/// doubleword64 L + A - P.
pub const R_PPC64_PLTREL64: usize = 46;
/// half16* S + A - .TOC.
pub const R_PPC64_TOC16: usize = 47;
/// half16 #lo(S + A - .TOC.).
pub const R_PPC64_TOC16_LO: usize = 48;
/// half16 #hi(S + A - .TOC.).
pub const R_PPC64_TOC16_HI: usize = 49;
/// half16 #ha(S + A - .TOC.).
pub const R_PPC64_TOC16_HA: usize = 50;
/// doubleword64 .TOC.
pub const R_PPC64_TOC: usize = 51;
/// half16* M + A.
pub const R_PPC64_PLTGOT16: usize = 52;
/// half16 #lo(M + A).
pub const R_PPC64_PLTGOT16_LO: usize = 53;
/// half16 #hi(M + A).
pub const R_PPC64_PLTGOT16_HI: usize = 54;
/// half16 #ha(M + A).
pub const R_PPC64_PLTGOT16_HA: usize = 55;

/// half16ds* (S + A) >> 2.
pub const R_PPC64_ADDR16_DS: usize = 56;
/// half16ds  #lo(S + A) >> 2.
pub const R_PPC64_ADDR16_LO_DS: usize = 57;
/// half16ds* (G + A) >> 2.
pub const R_PPC64_GOT16_DS: usize = 58;
/// half16ds  #lo(G + A) >> 2.
pub const R_PPC64_GOT16_LO_DS: usize = 59;
/// half16ds  #lo(L + A) >> 2.
pub const R_PPC64_PLT16_LO_DS: usize = 60;
/// half16ds* (R + A) >> 2.
pub const R_PPC64_SECTOFF_DS: usize = 61;
/// half16ds  #lo(R + A) >> 2.
pub const R_PPC64_SECTOFF_LO_DS: usize = 62;
/// half16ds* (S + A - .TOC.) >> 2.
pub const R_PPC64_TOC16_DS: usize = 63;
/// half16ds  #lo(S + A - .TOC.) >> 2.
pub const R_PPC64_TOC16_LO_DS: usize = 64;
/// half16ds* (M + A) >> 2.
pub const R_PPC64_PLTGOT16_DS: usize = 65;
/// half16ds  #lo(M + A) >> 2.
pub const R_PPC64_PLTGOT16_LO_DS: usize = 66;

/// PowerPC64 relocations defined for the TLS access ABI.
/// none	(sym+add)@tls
pub const R_PPC64_TLS: usize = 67;
/// doubleword64 (sym+add)@dtpmod
pub const R_PPC64_DTPMOD64: usize = 68;
/// half16*	(sym+add)@tprel
pub const R_PPC64_TPREL16: usize = 69;
/// half16	(sym+add)@tprel@l
pub const R_PPC64_TPREL16_LO: usize = 70;
/// half16	(sym+add)@tprel@h
pub const R_PPC64_TPREL16_HI: usize = 71;
/// half16	(sym+add)@tprel@ha
pub const R_PPC64_TPREL16_HA: usize = 72;
/// doubleword64 (sym+add)@tprel
pub const R_PPC64_TPREL64: usize = 73;
/// half16*	(sym+add)@dtprel
pub const R_PPC64_DTPREL16: usize = 74;
/// half16	(sym+add)@dtprel@l
pub const R_PPC64_DTPREL16_LO: usize = 75;
/// half16	(sym+add)@dtprel@h
pub const R_PPC64_DTPREL16_HI: usize = 76;
/// half16	(sym+add)@dtprel@ha
pub const R_PPC64_DTPREL16_HA: usize = 77;
/// doubleword64 (sym+add)@dtprel
pub const R_PPC64_DTPREL64: usize = 78;
/// half16*	(sym+add)@got@tlsgd
pub const R_PPC64_GOT_TLSGD16: usize = 79;
/// half16	(sym+add)@got@tlsgd@l
pub const R_PPC64_GOT_TLSGD16_LO: usize = 80;
/// half16	(sym+add)@got@tlsgd@h
pub const R_PPC64_GOT_TLSGD16_HI: usize = 81;
/// half16	(sym+add)@got@tlsgd@ha
pub const R_PPC64_GOT_TLSGD16_HA: usize = 82;
/// half16*	(sym+add)@got@tlsld
pub const R_PPC64_GOT_TLSLD16: usize = 83;
/// half16	(sym+add)@got@tlsld@l
pub const R_PPC64_GOT_TLSLD16_LO: usize = 84;
/// half16	(sym+add)@got@tlsld@h
pub const R_PPC64_GOT_TLSLD16_HI: usize = 85;
/// half16	(sym+add)@got@tlsld@ha
pub const R_PPC64_GOT_TLSLD16_HA: usize = 86;
/// half16ds*	(sym+add)@got@tprel
pub const R_PPC64_GOT_TPREL16_DS: usize = 87;
/// half16ds (sym+add)@got@tprel@l
pub const R_PPC64_GOT_TPREL16_LO_DS: usize = 88;
/// half16	(sym+add)@got@tprel@h
pub const R_PPC64_GOT_TPREL16_HI: usize = 89;
/// half16	(sym+add)@got@tprel@ha
pub const R_PPC64_GOT_TPREL16_HA: usize = 90;
/// half16ds*	(sym+add)@got@dtprel
pub const R_PPC64_GOT_DTPREL16_DS: usize = 91;
/// half16ds (sym+add)@got@dtprel@l
pub const R_PPC64_GOT_DTPREL16_LO_DS: usize = 92;
/// half16	(sym+add)@got@dtprel@h
pub const R_PPC64_GOT_DTPREL16_HI: usize = 93;
/// half16	(sym+add)@got@dtprel@ha
pub const R_PPC64_GOT_DTPREL16_HA: usize = 94;
/// half16ds*	(sym+add)@tprel
pub const R_PPC64_TPREL16_DS: usize = 95;
/// half16ds	(sym+add)@tprel@l
pub const R_PPC64_TPREL16_LO_DS: usize = 96;
/// half16	(sym+add)@tprel@higher
pub const R_PPC64_TPREL16_HIGHER: usize = 97;
/// half16	(sym+add)@tprel@highera
pub const R_PPC64_TPREL16_HIGHERA: usize = 98;
/// half16	(sym+add)@tprel@highest
pub const R_PPC64_TPREL16_HIGHEST: usize = 99;
/// half16	(sym+add)@tprel@highesta
pub const R_PPC64_TPREL16_HIGHESTA: usize = 100;
/// half16ds* (sym+add)@dtprel
pub const R_PPC64_DTPREL16_DS: usize = 101;
/// half16ds	(sym+add)@dtprel@l
pub const R_PPC64_DTPREL16_LO_DS: usize = 102;
/// half16	(sym+add)@dtprel@higher
pub const R_PPC64_DTPREL16_HIGHER: usize = 103;
/// half16	(sym+add)@dtprel@highera
pub const R_PPC64_DTPREL16_HIGHERA: usize = 104;
/// half16	(sym+add)@dtprel@highest
pub const R_PPC64_DTPREL16_HIGHEST: usize = 105;
/// half16	(sym+add)@dtprel@highesta
pub const R_PPC64_DTPREL16_HIGHESTA: usize = 106;
pub const R_PPC64_TLSGD: usize = 107;
pub const R_PPC64_TLSLD: usize = 108;
pub const R_PPC64_TOCSAVE: usize = 109;

pub const R_PPC64_ENTRY: usize = 118;

pub const R_PPC64_REL16: usize = 249;
pub const R_PPC64_REL16_LO: usize = 250;
pub const R_PPC64_REL16_HI: usize = 251;
pub const R_PPC64_REL16_HA: usize = 252;

/// Keep this the last entry.
pub const R_PPC64_NUM: usize = 253;
