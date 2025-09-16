// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/common_ansi.h`

/// Used in clock()
pub type clock_t = u32;
/// ptr1 - ptr2
pub type ptrdiff_t = isize;
/// byte count or error
pub type ssize_t = isize;
/// sizeof()
pub type size_t = usize;
/// Used in time()
pub type time_t = i64;
pub type clockid_t = i32;
pub type timer_t = i32;
pub type suseconds_t = i32;
pub type useconds_t = u32;
