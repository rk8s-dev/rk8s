// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_caddr_t.h`

use crate::c_char;

pub type caddr_t = *mut c_char;
