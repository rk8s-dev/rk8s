// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `i386/types.h`

#[cfg(target_pointer_width = "64")]
pub type register_t = i64;
#[cfg(target_pointer_width = "32")]
pub type register_t = i32;

/// These types are used for reserving the largest possible size.
pub type user_addr_t = u64;
pub type user_size_t = u64;
pub type user_ssize_t = i64;
pub type user_long_t = i64;
pub type user_ulong_t = u64;
pub type user_time_t = i64;
pub type user_off_t = i64;
pub const USER_ADDR_NULL: user_addr_t = 0;

/// This defines the size of syscall arguments after copying into the kernel:
pub type syscall_arg_t = u64;
