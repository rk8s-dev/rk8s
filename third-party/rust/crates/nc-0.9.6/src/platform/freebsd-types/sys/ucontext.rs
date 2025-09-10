// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/ucontext.h`

/// Used by swapcontext(3).
pub const UCF_SWAPPED: i32 = 0x00000001;
