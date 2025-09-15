// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/memfd.h`

use crate::{
    HUGETLB_FLAG_ENCODE_16GB, HUGETLB_FLAG_ENCODE_16MB, HUGETLB_FLAG_ENCODE_1GB,
    HUGETLB_FLAG_ENCODE_1MB, HUGETLB_FLAG_ENCODE_256MB, HUGETLB_FLAG_ENCODE_2GB,
    HUGETLB_FLAG_ENCODE_2MB, HUGETLB_FLAG_ENCODE_32MB, HUGETLB_FLAG_ENCODE_512KB,
    HUGETLB_FLAG_ENCODE_512MB, HUGETLB_FLAG_ENCODE_64KB, HUGETLB_FLAG_ENCODE_8MB,
    HUGETLB_FLAG_ENCODE_MASK, HUGETLB_FLAG_ENCODE_SHIFT,
};

/// flags for `memfd_create(2)` (unsigned int)
pub const MFD_CLOEXEC: u32 = 0x0001;
pub const MFD_ALLOW_SEALING: u32 = 0x0002;
pub const MFD_HUGETLB: u32 = 0x0004;

/// Huge page size encoding when `MFD_HUGETLB` is specified, and a huge page
/// size other than the default is desired.
///
/// See `hugetlb_encode.h`.
///
/// All known huge page size encodings are provided here. It is the responsibility
/// of the application to know which sizes are supported on the running system.
/// See `mmap(2)` man page for details.
pub const MFD_HUGE_SHIFT: i32 = HUGETLB_FLAG_ENCODE_SHIFT;
pub const MFD_HUGE_MASK: i32 = HUGETLB_FLAG_ENCODE_MASK;

pub const MFD_HUGE_64KB: usize = HUGETLB_FLAG_ENCODE_64KB;
pub const MFD_HUGE_512KB: usize = HUGETLB_FLAG_ENCODE_512KB;
pub const MFD_HUGE_1MB: usize = HUGETLB_FLAG_ENCODE_1MB;
pub const MFD_HUGE_2MB: usize = HUGETLB_FLAG_ENCODE_2MB;
pub const MFD_HUGE_8MB: usize = HUGETLB_FLAG_ENCODE_8MB;
pub const MFD_HUGE_16MB: usize = HUGETLB_FLAG_ENCODE_16MB;
pub const MFD_HUGE_32MB: usize = HUGETLB_FLAG_ENCODE_32MB;
pub const MFD_HUGE_256MB: usize = HUGETLB_FLAG_ENCODE_256MB;
pub const MFD_HUGE_512MB: usize = HUGETLB_FLAG_ENCODE_512MB;
pub const MFD_HUGE_1GB: usize = HUGETLB_FLAG_ENCODE_1GB;
pub const MFD_HUGE_2GB: usize = HUGETLB_FLAG_ENCODE_2GB;
pub const MFD_HUGE_16GB: usize = HUGETLB_FLAG_ENCODE_16GB;
