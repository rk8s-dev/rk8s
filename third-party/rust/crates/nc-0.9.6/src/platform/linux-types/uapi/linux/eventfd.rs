// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/eventfd.h`

use crate::{O_CLOEXEC, O_NONBLOCK};

pub const EFD_SEMAPHORE: i32 = 1 << 0;
pub const EFD_CLOEXEC: i32 = O_CLOEXEC;
pub const EFD_NONBLOCK: i32 = O_NONBLOCK;
