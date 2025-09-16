// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/pidfd.h`

use crate::{O_EXCL, O_NONBLOCK};

/// Flags for `pidfd_open()`.
pub const PIDFD_NONBLOCK: u32 = O_NONBLOCK as u32;
pub const PIDFD_THREAD: u32 = O_EXCL as u32;

/// Flags for `pidfd_send_signal()`.
pub const PIDFD_SIGNAL_THREAD: u32 = 1 << 0;
pub const PIDFD_SIGNAL_THREAD_GROUP: u32 = 1 << 1;
pub const PIDFD_SIGNAL_PROCESS_GROUP: u32 = 1 << 2;
