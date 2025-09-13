// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/quotactl.h`

use crate::{c_char, quotakey_t, quotaval_t, uintmax_t};

/// Size of random quota strings
pub const QUOTA_NAMELEN: usize = 32;

/// Structure for QUOTACTL_STAT.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct quotastat_t {
    pub qs_implname: [c_char; QUOTA_NAMELEN],
    pub qs_numidtypes: i32,
    pub qs_numobjtypes: i32,
    /// semantic restriction codes
    pub qs_restrictions: u32,
}

/// Structures for QUOTACTL_IDTYPESTAT and QUOTACTL_OBJTYPESTAT.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct quotaidtypestat_t {
    pub qis_name: [c_char; QUOTA_NAMELEN],
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct quotaobjtypestat_t {
    pub qos_name: [c_char; QUOTA_NAMELEN],
    pub qos_isbytes: i32,
}

/// Semi-opaque structure for cursors. This holds the cursor state in
/// userland; the size is exposed only to libquota, not to client code,
/// and is meant to be large enough to accommodate all likely future
/// expansion without being unduly bloated, as it will need to be
/// copied in and out for every call using it.
#[repr(C)]
pub struct quotakcursor_t {
    pub u: quotakcursor_union,
}

#[repr(C)]
pub union quotakcursor_union {
    pub qkc_space: [c_char; 64],
    __qkc_forcealign: uintmax_t,
}

/// Command codes.
pub const QUOTACTL_STAT: i32 = 0;
pub const QUOTACTL_IDTYPESTAT: i32 = 1;
pub const QUOTACTL_OBJTYPESTAT: i32 = 2;
pub const QUOTACTL_GET: i32 = 3;
pub const QUOTACTL_PUT: i32 = 4;
pub const QUOTACTL_DEL: i32 = 5;
pub const QUOTACTL_CURSOROPEN: i32 = 6;
pub const QUOTACTL_CURSORCLOSE: i32 = 7;
pub const QUOTACTL_CURSORSKIPIDTYPE: i32 = 8;
pub const QUOTACTL_CURSORGET: i32 = 9;
pub const QUOTACTL_CURSORATEND: i32 = 10;
pub const QUOTACTL_CURSORREWIND: i32 = 11;
pub const QUOTACTL_QUOTAON: i32 = 12;
pub const QUOTACTL_QUOTAOFF: i32 = 13;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_stat_t {
    pub qc_info: *mut quotastat_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_idtypestat_t {
    pub qc_idetype: i32,
    pub qc_info: *mut quotaidtypestat_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_objtypestat_t {
    pub qc_objtype: i32,
    pub qc_info: *mut quotaobjtypestat_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_get_t {
    pub qc_key: *const quotakey_t,
    pub qc_val: *mut quotaval_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_put_t {
    pub qc_key: *const quotakey_t,
    pub qc_val: *const quotaval_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_del_t {
    pub qc_key: *const quotakey_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_cursoropen_t {
    pub qc_cursor: *mut quotakcursor_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_cursorclose_t {
    pub qc_cursor: *mut quotakcursor_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_cursorskipidtype_t {
    pub qc_cursor: *mut quotakcursor_t,
    pub qc_idtype: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_cursorget_t {
    pub qc_cursor: *mut quotakcursor_t,
    pub qc_keys: *mut quotakey_t,
    pub qc_vals: *mut quotaval_t,
    pub qc_maxnum: u32,
    pub qc_ret: *mut u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_cursoratend_t {
    pub qc_cursor: *mut quotakcursor_t,
    // really boolean
    pub qc_ret: *mut i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_cursorrewind_t {
    pub qc_cursor: *mut quotakcursor_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_quotaon_t {
    pub qc_idtype: i32,
    pub qc_quotafile: *const c_char,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct quotactl_args_quotaoff_t {
    pub qc_idtype: i32,
}

#[repr(C)]
pub union quotactl_args_union {
    pub stat: quotactl_args_stat_t,
    pub idtypestat: quotactl_args_idtypestat_t,
    pub objtypestat: quotactl_args_objtypestat_t,

    pub get: quotactl_args_get_t,
    pub put: quotactl_args_put_t,
    pub del: quotactl_args_del_t,

    pub cursoropen: quotactl_args_cursoropen_t,
    pub cursorclose: quotactl_args_cursorclose_t,
    pub cursorskipidtype: quotactl_args_cursorskipidtype_t,
    pub cursorget: quotactl_args_cursorget_t,
    pub cursoratend: quotactl_args_cursoratend_t,
    pub cursorrewind: quotactl_args_cursorrewind_t,

    pub quotaon: quotactl_args_quotaon_t,
    pub quotaoff: quotactl_args_quotaoff_t,
}

/// Argument encoding.
#[repr(C)]
pub struct quotactl_args_t {
    pub qc_op: u32,
    pub u: quotactl_args_union,
}
