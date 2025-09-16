// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/timerfd.h`

use crate::{O_CLOEXEC, O_NONBLOCK};

/// CAREFUL: Check include/asm-generic/fcntl.h when defining
/// new flags, since they might collide with `O_*` ones.
///
/// We want to re-use `O_*` flags that couldn't possibly have a meaning
/// from eventfd, in order to leave a free define-space for
/// shared `O_*` flags.
pub const TFD_TIMER_ABSTIME: i32 = 1 << 0;
pub const TFD_TIMER_CANCEL_ON_SET: i32 = 1 << 1;
pub const TFD_CLOEXEC: i32 = O_CLOEXEC;
pub const TFD_NONBLOCK: i32 = O_NONBLOCK;

//pub const TFD_IOC_SET_TICKS: i32 = _IOW;('T', 0, __u64)
