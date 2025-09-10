// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/ars.h`

use crate::uintptr_t;

#[repr(C)]
pub struct ras_t {
    pub ras_next: *mut ras_t,
    pub ras_startaddr: uintptr_t,
    pub ras_endaddr: uintptr_t,
}

pub const RAS_INSTALL: i32 = 0;
pub const RAS_PURGE: i32 = 1;
pub const RAS_PURGE_ALL: i32 = 2;
