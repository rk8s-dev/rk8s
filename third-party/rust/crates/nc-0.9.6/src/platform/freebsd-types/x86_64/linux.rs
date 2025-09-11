// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `amd64/linux/linux.h`

use crate::off_t;

/// Provide a separate set of types for the Linux types.
pub type loff_t = off_t;
