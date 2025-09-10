// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/filedesc.h`

use crate::cap_rights_t;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct filecaps_t {
    /// per-descriptor capability rights
    pub fc_rights: cap_rights_t,

    /// per-descriptor allowed ioctls
    //TODO(Shaohua): Defined as *mut usize
    pub fc_ioctls: usize,

    /// fc_ioctls array size
    pub fc_nioctls: i16,

    /// per-descriptor allowed fcntls
    pub fc_fcntls: u32,
}

/// This structure is used for the management of descriptors.
///
/// It may be shared by multiple processes.
pub type NDSLOTTYPE = usize;

/// Per-process open flags.
///
/// auto-close on exec
pub const UF_EXCLOSE: i32 = 0x01;
