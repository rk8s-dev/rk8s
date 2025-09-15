// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/caprights.h`

/// The top two bits in the first element of the `cr_rights[]` array contain
/// total number of elements in the `array - 2`.
///
/// This means if those two bits are equal to 0, we have 2 array elements.
/// The top two bits in all remaining array elements should be 0.
/// The next five bits contain array index. Only one bit is used and bit position
/// in this five-bits range defines array index. This means there can be at most
/// five array elements.
pub const CAP_RIGHTS_VERSION_00: usize = 0;
// pub const CAP_RIGHTS_VERSION_01: usize = 1;
// pub const CAP_RIGHTS_VERSION_02: usize = 2;
// pub const CAP_RIGHTS_VERSION_03: usize = 3;
pub const CAP_RIGHTS_VERSION: usize = CAP_RIGHTS_VERSION_00;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct cap_rights_t {
    pub cr_rights: [u64; CAP_RIGHTS_VERSION_00 + 2],
}
