// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/signal.h`

pub const SS_ONSTACK: i32 = 1;
pub const SS_DISABLE: i32 = 2;

/// bit-flags
/// disable sas during sighandling
pub const SS_AUTODISARM: usize = 1 << 31;
/// mask for all `SS_xxx` flags
pub const SS_FLAG_BITS: usize = SS_AUTODISARM;
