// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From arch/include/powerpc/boot/elf.h

/// 32-bit ELF base types.
pub type Elf32_Addr_t = u32;
pub type Elf32_Half_t = u16;
pub type Elf32_Off_t = u32;
pub type Elf32_Sword_t = i32;
pub type Elf32_Word_t = u32;

/// 64-bit ELF base types.
pub type Elf64_Addr_t = u64;
pub type Elf64_Half_t = u16;
pub type Elf64_SHalf_t = i16;
pub type Elf64_Off_t = u64;
pub type Elf64_Sword_t = i32;
pub type Elf64_Word_t = u32;
pub type Elf64_Xword_t = u64;
pub type Elf64_Sxword_t = i64;

/// These constants are for the segment types stored in the image headers
pub const PT_NULL: i32 = 0;
pub const PT_LOAD: i32 = 1;
pub const PT_DYNAMIC: i32 = 2;
pub const PT_INTERP: i32 = 3;
pub const PT_NOTE: i32 = 4;
pub const PT_SHLIB: i32 = 5;
pub const PT_PHDR: i32 = 6;
/// Thread local storage segment
pub const PT_TLS: i32 = 7;
/// OS-specific
pub const PT_LOOS: i32 = 0x60000000;
/// OS-specific
pub const PT_HIOS: i32 = 0x6fffffff;
pub const PT_LOPROC: i32 = 0x70000000;
pub const PT_HIPROC: i32 = 0x7fffffff;
pub const PT_GNU_EH_FRAME: i32 = 0x6474e550;

pub const PT_GNU_STACK: i32 = PT_LOOS + 0x474e551;

/// These constants define the different elf file types
pub const ET_NONE: i32 = 0;
pub const ET_REL: i32 = 1;
pub const ET_EXEC: i32 = 2;
pub const ET_DYN: i32 = 3;
pub const ET_CORE: i32 = 4;
pub const ET_LOPROC: i32 = 0xff00;
pub const ET_HIPROC: i32 = 0xffff;

/// These constants define the various ELF target machines
pub const EM_NONE: i32 = 0;
/// PowerPC
pub const EM_PPC: i32 = 20;
/// PowerPC64
pub const EM_PPC64: i32 = 21;

pub const EI_NIDENT: usize = 16;

#[repr(C)]
pub struct elf32_hdr_t {
    pub e_ident: [u8; EI_NIDENT],
    pub e_type: Elf32_Half_t,
    pub e_machine: Elf32_Half_t,
    pub e_version: Elf32_Word_t,
    /// Entry point
    pub e_entry: Elf32_Addr_t,
    pub e_phoff: Elf32_Off_t,
    pub e_shoff: Elf32_Off_t,
    pub e_flags: Elf32_Word_t,
    pub e_ehsize: Elf32_Half_t,
    pub e_phentsize: Elf32_Half_t,
    pub e_phnum: Elf32_Half_t,
    pub e_shentsize: Elf32_Half_t,
    pub e_shnum: Elf32_Half_t,
    pub e_shstrndx: Elf32_Half_t,
}

pub type Elf32_Ehdr_t = elf32_hdr_t;

#[repr(C)]
pub struct elf64_hdr_t {
    /// ELF "magic number"
    pub e_ident: [u8; 16],
    pub e_type: Elf64_Half_t,
    pub e_machine: Elf64_Half_t,
    pub e_version: Elf64_Word_t,
    /// Entry point virtual address
    pub e_entry: Elf64_Addr_t,
    /// Program header table file offset
    pub e_phoff: Elf64_Off_t,
    /// Section header table file offset
    pub e_shoff: Elf64_Off_t,
    pub e_flags: Elf64_Word_t,
    pub e_ehsize: Elf64_Half_t,
    pub e_phentsize: Elf64_Half_t,
    pub e_phnum: Elf64_Half_t,
    pub e_shentsize: Elf64_Half_t,
    pub e_shnum: Elf64_Half_t,
    pub e_shstrndx: Elf64_Half_t,
}

pub type Elf64_Ehdr_t = elf64_hdr_t;

/// These constants define the permissions on sections in the program header, p_flags.
pub const PF_R: i32 = 0x4;
pub const PF_W: i32 = 0x2;
pub const PF_X: i32 = 0x1;

#[repr(C)]
pub struct elf32_phdr_t {
    pub p_type: Elf32_Word_t,
    pub p_offset: Elf32_Off_t,
    pub p_vaddr: Elf32_Addr_t,
    pub p_paddr: Elf32_Addr_t,
    pub p_filesz: Elf32_Word_t,
    pub p_memsz: Elf32_Word_t,
    pub p_flags: Elf32_Word_t,
    pub p_align: Elf32_Word_t,
}

pub type Elf32_Phdr_t = elf32_phdr_t;

#[repr(C)]
pub struct elf64_phdr_t {
    pub p_type: Elf64_Word_t,
    pub p_flags: Elf64_Word_t,
    /// Segment file offset
    pub p_offset: Elf64_Off_t,
    /// Segment virtual address
    pub p_vaddr_t: Elf64_Addr_t,
    /// Segment physical address
    pub p_paddr: Elf64_Addr_t,
    /// Segment size in file
    pub p_filesz: Elf64_Xword_t,
    /// Segment size in memory
    pub p_memsz: Elf64_Xword_t,
    /// Segment alignment, file & memory
    pub p_align: Elf64_Xword_t,
}

pub type Elf64_Phdr_t = elf64_phdr_t;

/// e_ident[] indexes
pub const EI_MAG0: i32 = 0;
pub const EI_MAG1: i32 = 1;
pub const EI_MAG2: i32 = 2;
pub const EI_MAG3: i32 = 3;
pub const EI_CLASS: i32 = 4;
pub const EI_DATA: i32 = 5;
pub const EI_VERSION: i32 = 6;
pub const EI_OSABI: i32 = 7;
pub const EI_PAD: i32 = 8;

/// EI_MAG
pub const ELFMAG0: u8 = 0x7f;
pub const ELFMAG1: u8 = b'E';
pub const ELFMAG2: u8 = b'L';
pub const ELFMAG3: u8 = b'F';
pub const ELFMAG: &str = "ELF";
pub const SELFMAG: i32 = 4;

/// EI_CLASS
pub const ELFCLASSNONE: i32 = 0;
pub const ELFCLASS32: i32 = 1;
pub const ELFCLASS64: i32 = 2;
pub const ELFCLASSNUM: i32 = 3;

/// e_ident[EI_DATA]
pub const ELFDATANONE: i32 = 0;
pub const ELFDATA2LSB: i32 = 1;
pub const ELFDATA2MSB: i32 = 2;

/// e_version, EI_VERSION
pub const EV_NONE: i32 = 0;
pub const EV_CURRENT: i32 = 1;
pub const EV_NUM: i32 = 2;

pub const ELFOSABI_NONE: i32 = 0;
pub const ELFOSABI_LINUX: i32 = 3;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct elf_info_t {
    pub loadsize: usize,
    pub memsize: usize,
    pub elfoffset: usize,
}
