// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/powerpc/include/uapi/asm/types.h`

#[repr(C)]
pub struct __vector128_t {
    pub u: [u32; 4],
}
