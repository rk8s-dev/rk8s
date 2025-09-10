// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_ino_t.h`

/// inode number
///
/// Used for 64 bit inodes
pub type ino_t = u64;

// Used for 32 bit inodes
// TODO(Shaohua): Check __DARWIN_64_BIT_INO_T
//pub type ino_t = u32;
