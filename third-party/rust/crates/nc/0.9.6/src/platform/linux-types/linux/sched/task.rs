// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/sched/task.h`

#[repr(C)]
pub struct kernel_clone_args_t<'a> {
    pub flags: u32,
    pub exit_signal: i32,
    pub parent_tid: Option<&'a mut i32>,
    pub child_tid: Option<&'a mut i32>,
    pub stack: usize,
    pub stack_size: usize,
    pub tls: usize,
}
