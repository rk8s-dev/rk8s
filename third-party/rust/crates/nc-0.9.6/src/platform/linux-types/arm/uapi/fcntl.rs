// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/arm/include/uapi/asm/fcntl.h`

/// must be a directory
pub const O_DIRECTORY: i32 = 0o40_000;
/// don't follow links
pub const O_NOFOLLOW: i32 = 0o100_000;
/// direct disk access hint - currently ignored
pub const O_DIRECT: i32 = 0o200_000;
pub const O_LARGEFILE: i32 = 0o400_000;
