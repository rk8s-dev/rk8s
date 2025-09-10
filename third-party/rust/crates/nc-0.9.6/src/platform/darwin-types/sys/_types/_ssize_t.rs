// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_ssize_t.h`

use crate::__darwin_ssize_t;

pub type ssize_t = __darwin_ssize_t;
