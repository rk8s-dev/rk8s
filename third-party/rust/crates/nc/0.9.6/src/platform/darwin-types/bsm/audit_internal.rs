// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `bsdm/audit_internal.h`

use crate::size_t;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct au_token_t {
    pub t_data: u8,
    pub len: size_t,
    tokens: *mut au_token_t,
}
