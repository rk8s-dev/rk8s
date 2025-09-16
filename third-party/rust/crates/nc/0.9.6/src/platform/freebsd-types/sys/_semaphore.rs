// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_semaphore.h`

use crate::intptr_t;

pub type semid_t = intptr_t;

pub const SEM_VALUE_MAX: i32 = i32::MAX;
