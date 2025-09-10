// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/common_int_mwgwtypes.h`
//!
//!  7.18.1 Integer types

use crate::{__intmax_type__, __uintmax_type__};

// 7.18.1.2 Minimum-width integer types
// 7.18.1.3 Fastest minimum-width integer types
/// 7.18.1.5 Greatest-width integer types
pub type intmax_t = __intmax_type__;
pub type uintmax_t = __uintmax_type__;
