// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_cpuset.h`

use crate::__bitset_words;

pub const CPU_MAXSIZE: usize = 256;
pub const CPU_SETSIZE: usize = CPU_MAXSIZE;

#[derive(Clone)]
#[repr(C)]
pub struct _cpuset_t {
    pub __bits: [isize; __bitset_words(CPU_SETSIZE)],
}

pub type cpuset_t = _cpuset_t;
