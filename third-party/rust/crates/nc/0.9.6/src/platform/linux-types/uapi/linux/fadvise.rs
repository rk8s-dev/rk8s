// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/fadvise.h`

/// No further special treatment.
pub const POSIX_FADV_NORMAL: i32 = 0;

/// Expect random page references.
pub const POSIX_FADV_RANDOM: i32 = 1;

/// Expect sequential page references.
pub const POSIX_FADV_SEQUENTIAL: i32 = 2;

/// Will need these pages.
pub const POSIX_FADV_WILLNEED: i32 = 3;

/// The advise values for POSIX_FADV_DONTNEED and POSIX_ADV_NOREUSE
/// for s390-64 differ from the values for the rest of the world.
#[cfg(target_arch = "s390x")]
/// Don't need these pages.
pub const POSIX_FADV_DONTNEED: i32 = 6;

#[cfg(target_arch = "s390x")]
/// Data will be accessed once.
pub const POSIX_FADV_NOREUSE: i32 = 7;

#[cfg(not(target_arch = "s390x"))]
/// Don't need these pages.
pub const POSIX_FADV_DONTNEED: i32 = 4;

#[cfg(not(target_arch = "s390x"))]
/// Data will be accessed once.
pub const POSIX_FADV_NOREUSE: i32 = 5;
