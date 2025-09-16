// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/arm/include/asm/posix_types.h`
//!
//! This file is generally used by user-level software, so you need to
//! be a little careful about namespace pollution etc.  Also, we cannot
//! assume GCC is being used.

pub type __kernel_mode_t = u16;

pub type __kernel_ipc_pid_t = u16;

pub type __kernel_uid_t = u16;
pub type __kernel_gid_t = u16;

pub type __kernel_old_dev_t = u16;
