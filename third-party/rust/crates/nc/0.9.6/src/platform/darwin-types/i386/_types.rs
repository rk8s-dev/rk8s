// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `i386/_types.h`

pub type __darwin_intptr_t = isize;
pub type __darwin_natural_t = usize;

/// `ct_rune_t`
pub type __darwin_ct_rune_t = i32;

#[cfg(target_pointer_width = "64")]
/// ptr1 - ptr2
pub type __darwin_ptrdiff_t = isize;
#[cfg(target_pointer_width = "32")]
/// ptr1 - ptr2
pub type __darwin_ptrdiff_t = i32;

/// sizeof()
pub type __darwin_size_t = usize;

/// `wchar_t`
pub type __darwin_wchar_t = __darwin_ct_rune_t;

/// `rune_t`
pub type __darwin_rune_t = __darwin_wchar_t;

/// `wint_t`
pub type __darwin_wint_t = __darwin_ct_rune_t;

/// clock()
pub type __darwin_clock_t = usize;
/// `socklen_t` (duh)
pub type __darwin_socklen_t = u32;
/// byte count or error
pub type __darwin_ssize_t = isize;
/// time()
pub type __darwin_time_t = isize;
