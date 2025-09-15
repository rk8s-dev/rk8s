// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/splice.h`

/// Flags passed in from splice/tee/vmsplice
/// move pages instead of copying
pub const SPLICE_F_MOVE: u32 = 0x01;

/// don't block on the pipe splicing (but we may still block on the fd we splice from/to, of course
pub const SPLICE_F_NONBLOCK: u32 = 0x02;

/// expect more data
pub const SPLICE_F_MORE: u32 = 0x04;

/// pages passed in are a gift
pub const SPLICE_F_GIFT: u32 = 0x08;

pub const SPLICE_F_ALL: u32 = SPLICE_F_MOVE | SPLICE_F_NONBLOCK | SPLICE_F_MORE | SPLICE_F_GIFT;
