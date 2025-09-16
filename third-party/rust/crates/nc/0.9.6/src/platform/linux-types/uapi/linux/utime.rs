// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/utime.h`

use crate::time_t;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct utimbuf_t {
    pub actime: time_t,
    pub modtime: time_t,
}
