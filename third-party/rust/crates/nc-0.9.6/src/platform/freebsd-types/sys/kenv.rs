// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/kenv.h`

/// Constants for the kenv(2) syscall
pub const KENV_GET: i32 = 0;
pub const KENV_SET: i32 = 1;
pub const KENV_UNSET: i32 = 2;
pub const KENV_DUMP: i32 = 3;
pub const KENV_DUMP_LOADER: i32 = 4;
pub const KENV_DUMP_STATIC: i32 = 5;

/// Maximum name length (for the syscall)
pub const KENV_MNAMELEN: i32 = 128;
/// Maximum value length (for the syscall)
pub const KENV_MVALLEN: i32 = 128;
