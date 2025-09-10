// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

use crate::{clone, pid_t, Errno, SIGCHLD};

pub unsafe fn fork() -> Result<pid_t, Errno> {
    clone(SIGCHLD as usize, core::ptr::null(), None, None, None)
}
