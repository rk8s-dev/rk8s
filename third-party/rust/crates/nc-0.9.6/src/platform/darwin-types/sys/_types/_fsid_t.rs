// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

/// file system id type
#[repr(C)]
#[derive(Debug, Clone)]
pub struct fsid_t {
    pub val: [i32; 2],
}
