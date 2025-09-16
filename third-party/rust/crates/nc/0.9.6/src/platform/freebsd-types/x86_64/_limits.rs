// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `x86/include/_limits.h`

use crate::{off_t, quad_t, size_t, ssize_t, u_quad_t};

/// According to ANSI (section 2.2.4.2), the values below must be usable by
/// #if preprocessing directives.  Additionally, the expression must have the
/// same type as would an expression that is an object of the corresponding
/// type converted according to the integral promotions.  The subtraction for
/// `INT_MIN`, etc., is so the value is not unsigned; e.g., 0x80000000 is an
/// unsigned int for 32-bit two's complement ANSI compilers (section 3.1.3.2).
/// number of bits in a char
pub const CHAR_BIT: usize = 8;

/// max value for a signed char
pub const SCHAR_MAX: i8 = 0x7f;

/// min value for a signed char
pub const SCHAR_MIN: i8 = -0x7f - 1;

/// max value for an unsigned char
pub const UCHAR_MAX: u8 = 0xff;

/// max value for an unsigned short
pub const USHRT_MAX: u16 = 0xffff;

/// max value for a short
pub const SHRT_MAX: i16 = 0x7fff;

/// min value for a short
pub const SHRT_MIN: i16 = -0x7fff - 1;

/// max value for an unsigned int
pub const UINT_MAX: u32 = 0xffff_ffff;

/// max value for an int
pub const INT_MAX: i32 = 0x7fff_ffff;

/// min value for an int
pub const INT_MIN: i32 = -0x7fff_ffff - 1;

/// max for an unsigned long
pub const ULONG_MAX: usize = 0xffff_ffff_ffff_ffff;

/// max for a long
pub const LONG_MAX: isize = 0x7fff_ffff_ffff_ffff;

/// min for a long
pub const LONG_MIN: isize = -0x7fff_ffff_ffff_ffff - 1;

/// max value for an unsigned long long
pub const ULLONG_MAX: u64 = 0xffff_ffff_ffff_ffff;

/// max value for a long long
pub const LLONG_MAX: i64 = 0x7fff_ffff_ffff_ffff;

/// min for a long long
pub const LLONG_MIN: i64 = -0x7fff_ffff_ffff_ffff - 1;

/// max value for a `ssize_t`
pub const SSIZE_MAX: ssize_t = LONG_MAX;

/// max value for a `size_t`
pub const SIZE_T_MAX: size_t = ULONG_MAX;

/// max value for an `off_t`
pub const OFF_MAX: off_t = LONG_MAX as off_t;

/// min value for an `off_t`
pub const OFF_MIN: off_t = LONG_MIN as off_t;

/// Quads and longs are the same on the amd64. Ensure they stay in sync.
/// max value for a `u_quad_t`
pub const UQUAD_MAX: u_quad_t = ULONG_MAX as u_quad_t;

/// max value for a `quad_t`
pub const QUAD_MAX: quad_t = LONG_MAX as quad_t;

/// min value for a `quad_t`
pub const QUAD_MIN: quad_t = LONG_MIN as quad_t;

pub const LONG_BIT: usize = 64;

pub const WORD_BIT: usize = 32;

/// Minimum signal stack size.
pub const __MINSIGSTKSZ: usize = 512 * 4;
