// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/asm-generic/bitsperlong.h`

#[cfg(target_pointer_width = "64")]
pub const BITS_PER_LONG: usize = 64;

#[cfg(target_pointer_width = "32")]
pub const BITS_PER_LONG: usize = 32;

pub const BITS_PER_LONG_LONG: usize = 64;
