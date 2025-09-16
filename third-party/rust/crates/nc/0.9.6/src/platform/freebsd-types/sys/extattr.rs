// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/extattr.h`

use crate::NAME_MAX;

/// Defined name spaces for extended attributes.
///
/// Numeric constants are passed via system calls, but a user-friendly string
/// is also defined.
pub const EXTATTR_NAMESPACE_EMPTY: i32 = 0x00000000;
pub const EXTATTR_NAMESPACE_EMPTY_STRING: &str = "empty";
pub const EXTATTR_NAMESPACE_USER: i32 = 0x00000001;
pub const EXTATTR_NAMESPACE_USER_STRING: &str = "user";
pub const EXTATTR_NAMESPACE_SYSTEM: i32 = 0x00000002;
pub const EXTATTR_NAMESPACE_SYSTEM_STRING: &str = "system";

/// The following macro is designed to initialize an array that maps
/// extended-attribute namespace values to their names, e.g.:
///
/// char *extattr_namespace_names[] = EXTATTR_NAMESPACE_NAMES;
pub const EXTATTR_NAMESPACE_NAMES: &[&str] = &[
    EXTATTR_NAMESPACE_EMPTY_STRING,
    EXTATTR_NAMESPACE_USER_STRING,
    EXTATTR_NAMESPACE_SYSTEM_STRING,
];

pub const EXTATTR_MAXNAMELEN: usize = NAME_MAX;
