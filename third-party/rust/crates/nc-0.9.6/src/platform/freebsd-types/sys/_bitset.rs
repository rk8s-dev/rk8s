// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_bitset.h`
use core::mem::size_of;

/// Macros addressing word and bit within it, tuned to make compiler
/// optimize cases when SETSIZE fits into single machine word.
pub const _BITSET_BITS: usize = size_of::<isize>() * 8;

#[inline]
#[must_use]
pub const fn __howmany(x: usize, y: usize) -> usize {
    (x + (y - 1)) / y
}

#[inline]
#[must_use]
pub const fn __bitset_words(s: usize) -> usize {
    __howmany(s, _BITSET_BITS)
}

//#define	__BITSET_DEFINE(_t, _s)						\
//struct _t {								\
//        long    __bits[__bitset_words((_s))];				\
//}
