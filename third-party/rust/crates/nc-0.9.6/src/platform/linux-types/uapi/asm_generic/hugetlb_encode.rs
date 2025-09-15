// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/hugetlb_encode.h`

//! Several system calls take a flag to request "hugetlb" huge pages.
//! Without further specification, these system calls will use the
//! system's default huge page size.  If a system supports multiple
//! huge page sizes, the desired huge page size can be specified in
//! bits `[26:31]` of the flag arguments.  The value in these 6 bits
//! will encode the log2 of the huge page size.
//!
//! The following definitions are associated with this huge page size
//! encoding in flag arguments.  System call specific header files
//! that use this encoding should include this file.  They can then
//! provide definitions based on these with their own specific prefix.
//! for example:
//! `#define MAP_HUGE_SHIFT HUGETLB_FLAG_ENCODE_SHIFT`

pub const HUGETLB_FLAG_ENCODE_SHIFT: i32 = 26;
pub const HUGETLB_FLAG_ENCODE_MASK: i32 = 0x3f;

pub const HUGETLB_FLAG_ENCODE_64KB: usize = 16 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_512KB: usize = 19 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_1MB: usize = 20 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_2MB: usize = 21 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_8MB: usize = 23 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_16MB: usize = 24 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_32MB: usize = 25 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_256MB: usize = 28 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_512MB: usize = 29 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_1GB: usize = 30 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_2GB: usize = 31 << HUGETLB_FLAG_ENCODE_SHIFT;
pub const HUGETLB_FLAG_ENCODE_16GB: usize = 34 << HUGETLB_FLAG_ENCODE_SHIFT;
