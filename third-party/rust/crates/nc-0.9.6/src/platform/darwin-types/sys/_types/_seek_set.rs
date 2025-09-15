// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_o_seek_set.h`

/// whence values for lseek(2)
/// set file offset to offset
pub const SEEK_SET: i32 = 0;
/// set file offset to current plus offset
pub const SEEK_CUR: i32 = 1;
/// set file offset to EOF plus offset
pub const SEEK_END: i32 = 2;

/// set file offset to the start of the next hole greater than or equal to the supplied offset
pub const SEEK_HOLE: i32 = 3;

/// set file offset to the start of the next non-hole file region greater than or equal to the supplied offset
pub const SEEK_DATA: i32 = 4;
