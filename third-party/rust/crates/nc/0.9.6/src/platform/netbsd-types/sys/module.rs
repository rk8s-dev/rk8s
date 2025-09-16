// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/module.h`

pub const MAXMODNAME: usize = 32;
pub const MAXMODDEPS: usize = 10;

/// Module classes, provided only for system boot and module validation.
#[repr(C)]
pub enum modclass_t {
    MODULE_CLASS_ANY,
    MODULE_CLASS_MISC,
    MODULE_CLASS_VFS,
    MODULE_CLASS_DRIVER,
    MODULE_CLASS_EXEC,
    MODULE_CLASS_SECMODEL,
    MODULE_CLASS_BUFQ,
    MODULE_CLASS_MAX,
}

/// Module sources: where did it come from?
#[repr(C)]
pub enum modsrc_t {
    MODULE_SOURCE_KERNEL,
    MODULE_SOURCE_BOOT,
    MODULE_SOURCE_FILESYS,
}

/// Commands passed to module control routine.
#[repr(C)]
pub enum modcmd_t {
    /// mandatory
    MODULE_CMD_INIT,

    /// mandatory
    MODULE_CMD_FINI,

    /// optional
    MODULE_CMD_STAT,

    /// optional
    MODULE_CMD_AUTOUNLOAD,
}
