// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `common/lib/libc/sys/cpuset.c`

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct cpuset_t {
    pub bits: [u32; 1],
}
