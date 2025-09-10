// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_domainset.h`

use crate::__bitset_words;

pub const DOMAINSET_MAXSIZE: usize = 256;
pub const DOMAINSET_SETSIZE: usize = DOMAINSET_MAXSIZE;

#[derive(Clone)]
#[repr(C)]
pub struct _domainset_t {
    pub __bits: [isize; __bitset_words(DOMAINSET_SETSIZE)],
}

pub type domainset_t = _domainset_t;
