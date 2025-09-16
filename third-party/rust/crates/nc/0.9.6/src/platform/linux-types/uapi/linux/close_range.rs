// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `uapi/linux/close_range.h`

/// Unshare the file descriptor table before closing file descriptors.
pub const CLOSE_RANGE_UNSHARE: u32 = 1 << 1;

/// Set the `FD_CLOEXEC` bit instead of closing the file descriptor.
pub const CLOSE_RANGE_CLOEXEC: u32 = 1 << 2;
