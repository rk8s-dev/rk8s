// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `x86/include/_types.h`

pub const __NO_STRICT_ALIGNMENT: bool = true;

/// Standard type definitions.
///
/// See [man#arch](https://www.freebsd.org/cgi/man.cgi?query=arch&sektion=7&format=html)
/// clock()...
pub type clock_t = i32;
pub type critical_t = i64;

pub type int_fast8_t = i32;
pub type int_fast16_t = i32;
pub type int_fast32_t = i32;
pub type int_fast64_t = i64;

pub type register_t = i64;

/// segment size (in pages).
pub type segsz_t = i64;

/// time()...
pub type time_t = i64;

pub type uint_fast8_t = u32;
pub type uint_fast16_t = u32;
pub type uint_fast32_t = u32;
pub type uint_fast64_t = u64;
pub type u_register_t = u64;
pub type vm_paddr_t = u64;
pub type wchar_t = i32;

/// min value for a `wchar_t`
pub const WCHAR_MIN: i32 = i32::MIN;

/// max value for a `wchar_t`
pub const WCHAR_MAX: i32 = i32::MAX;
