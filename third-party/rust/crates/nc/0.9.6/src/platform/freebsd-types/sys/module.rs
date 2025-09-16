// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/module.h`

use crate::{c_char, MAXPATHLEN};

/// Module metadata types
/// argument is a module name
pub const MDT_DEPEND: i32 = 1;
/// module declaration
pub const MDT_MODULE: i32 = 2;
/// module version(s)
pub const MDT_VERSION: i32 = 3;
/// Plug and play hints record
pub const MDT_PNP_INFO: i32 = 4;

/// version of metadata structure
pub const MDT_STRUCT_VERSION: i32 = 1;
pub const MDT_SETNAME: &str = "modmetadata_set";

#[repr(C)]
pub enum modeventtype_t {
    MOD_LOAD,
    MOD_UNLOAD,
    MOD_SHUTDOWN,
    MOD_QUIESCE,
}

pub type modeeventhand_t = fn(usize /* module */, modeventtype_t, usize) -> i32;

/// Struct for registering modules statically via SYSINIT.
#[repr(C)]
#[derive(Debug)]
pub struct moduledata_t {
    /// module name
    pub name: *const c_char,
    /// event handler
    pub evhand: modeeventhand_t,
    /// extra data
    priv_: usize,
}

/// A module can use this to report module specific data to the user via kldstat(2).
#[repr(C)]
#[derive(Clone, Copy)]
pub union modspecific_t {
    intval: i32,
    uintval: u32,
    longval: isize,
    ulongval: usize,
}

/// Module dependency declaration
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct mod_depend {
    pub md_ver_minimum: i32,
    pub md_ver_preferred: i32,
    pub md_ver_maximum: i32,
}

/// Module version declaration
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct mod_version_t {
    pub mv_version: i32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct mod_metadata_t {
    /// structure version MDTV_*
    pub md_version: i32,
    /// type of entry MDT_*
    pub md_type: i32,
    /// specific data
    pub md_data: usize,
    /// common string label
    pub md_cval: *const c_char,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct mod_pnp_match_info_t {
    /// Description of the table
    pub descr: *const c_char,
    /// Name of the bus for this table
    pub bus: *const c_char,
    /// Pointer to pnp table
    pub table: usize,
    /// Length of each entry in the table (may be longer than descr describes).
    pub entry_len: i32,
    /// Number of entries in the table
    pub num_entry: i32,
}

pub const MAXMODNAMEV1V2: usize = 32;
pub const MAXMODNAMEV3: usize = MAXPATHLEN;
pub const MAXMODNAME: usize = MAXMODNAMEV3;

#[repr(C)]
#[derive(Clone)]
pub struct module_stat_t {
    /// set to sizeof(struct module_stat)
    pub version: i32,
    pub name: [c_char; MAXMODNAME],
    pub refs: i32,
    pub id: i32,
    pub data: modspecific_t,
}
