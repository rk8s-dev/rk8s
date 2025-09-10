// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/ioprio.h`

#![allow(clippy::module_name_repetitions)]

/// Gives us 8 prio classes with 13-bits of data for each class
pub const IOPRIO_CLASS_SHIFT: u8 = 13;
pub const IOPRIO_PRIO_MASK: i32 = (1 << IOPRIO_CLASS_SHIFT) - 1;

#[inline]
#[must_use]
pub const fn ioprio_prio_class(mask: i32) -> i32 {
    mask >> IOPRIO_CLASS_SHIFT
}

#[inline]
#[must_use]
pub const fn ioprio_prio_data(mask: i32) -> i32 {
    mask & IOPRIO_PRIO_MASK
}

#[inline]
#[must_use]
pub const fn ioprio_prio_value(class_: i32, data: i32) -> i32 {
    (class_ << IOPRIO_CLASS_SHIFT) | data
}

#[inline]
#[must_use]
pub const fn ioprio_valid(mask: i32) -> bool {
    ioprio_prio_class(mask) != IOPRIO_CLASS_NONE
}

/// These are the io priority groups as implemented by CFQ.
///
/// RT is the realtime class, it always gets premium service.
/// BE is the best-effort scheduling class, the default for any process.
/// IDLE is the idle scheduling class, it is only served when no one else is using the disk.
pub const IOPRIO_CLASS_NONE: i32 = 0;
pub const IOPRIO_CLASS_RT: i32 = 1;
pub const IOPRIO_CLASS_BE: i32 = 2;
pub const IOPRIO_CLASS_IDLE: i32 = 3;

/// 8 best effort priority levels are supported
pub const IOPRIO_BE_NR: i32 = 8;

pub const IOPRIO_WHO_PROCESS: i32 = 1;
pub const IOPRIO_WHO_PGRP: i32 = 2;
pub const IOPRIO_WHO_USER: i32 = 3;

/// Fallback BE priority
pub const IOPRIO_NORM: i32 = 4;
