// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/linker.h`

use crate::{c_char, caddr_t, size_t, MAXPATHLEN};

/// Module information subtypes
/// End of list
pub const MODINFO_END: i32 = 0x0000;
/// Name of module (string)
pub const MODINFO_NAME: i32 = 0x0001;
/// Type of module (string)
pub const MODINFO_TYPE: i32 = 0x0002;
/// Loaded address
pub const MODINFO_ADDR: i32 = 0x0003;
/// Size of module
pub const MODINFO_SIZE: i32 = 0x0004;
/// Has been deleted
pub const MODINFO_EMPTY: i32 = 0x0005;
/// Parameters string
pub const MODINFO_ARGS: i32 = 0x0006;
/// Module-specfic
pub const MODINFO_METADATA: i32 = 0x8000;

/// a.out exec header
pub const MODINFOMD_AOUTEXEC: i32 = 0x0001;
/// ELF header
pub const MODINFOMD_ELFHDR: i32 = 0x0002;
/// start of symbols
pub const MODINFOMD_SSYM: i32 = 0x0003;
/// end of symbols
pub const MODINFOMD_ESYM: i32 = 0x0004;
/// _DYNAMIC pointer
pub const MODINFOMD_DYNAMIC: i32 = 0x0005;
/// MB2 header info
pub const MODINFOMD_MB2HDR: i32 = 0x0006;

// These values are MD on PowerPC
#[cfg(target_arch = "powerpc64")]
/// envp[]
pub const MODINFOMD_ENVP: i32 = 0x0006;
#[cfg(target_arch = "powerpc64")]
/// boothowto
pub const MODINFOMD_HOWTO: i32 = 0x0007;
#[cfg(target_arch = "powerpc64")]
/// kernend
pub const MODINFOMD_KERNEND: i32 = 0x0008;

/// section header table
pub const MODINFOMD_SHDR: i32 = 0x0009;
/// address of .ctors
pub const MODINFOMD_CTORS_ADDR: i32 = 0x000a;
/// size of .ctors
pub const MODINFOMD_CTORS_SIZE: i32 = 0x000b;
/// Firmware dependent handle
pub const MODINFOMD_FW_HANDLE: i32 = 0x000c;
/// Crypto key intake buffer
pub const MODINFOMD_KEYBUF: i32 = 0x000d;
/// Console font
pub const MODINFOMD_FONT: i32 = 0x000e;
/// don't copy this metadata to the kernel
pub const MODINFOMD_NOCOPY: i32 = 0x8000;

/// linker.hints file version
pub const LINKER_HINTS_VERSION: i32 = 1;
/// Allow at most 1MB for linker.hints
pub const LINKER_HINTS_MAX: i32 = 1 << 20;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct kld_file_stat_t {
    /// set to sizeof(struct kld_file_stat)
    pub version: i32,
    pub name: [c_char; MAXPATHLEN],
    pub refs: i32,
    pub id: i32,
    /// load address
    pub address: caddr_t,
    /// size in bytes
    pub size: size_t,
    pub pathname: [c_char; MAXPATHLEN],
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct kld_sym_lookup_t {
    /// set to sizeof(struct kld_sym_lookup)
    pub version: i32,
    /// Symbol name we are looking up
    pub symname: *const c_char,
    pub symvalue: usize,
    pub symsize: size_t,
}

pub const KLDSYM_LOOKUP: i32 = 1;

/// Flags for kldunloadf() and linker_file_unload()
pub const LINKER_UNLOAD_NORMAL: i32 = 0;
pub const LINKER_UNLOAD_FORCE: i32 = 1;
