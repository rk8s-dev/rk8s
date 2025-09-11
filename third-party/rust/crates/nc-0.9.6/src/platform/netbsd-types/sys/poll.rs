// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/poll.h`

pub type nfds_t = u32;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct pollfd_t {
    /// file descriptor
    pub fd: i32,
    /// events to look for
    pub events: i16,
    /// events returned
    pub revents: i16,
}

/// Testable events (may be specified in events field).
pub const POLLIN: i32 = 0x0001;
pub const POLLPRI: i32 = 0x0002;
pub const POLLOUT: i32 = 0x0004;
pub const POLLRDNORM: i32 = 0x0040;
pub const POLLWRNORM: i32 = POLLOUT;
pub const POLLRDBAND: i32 = 0x0080;
pub const POLLWRBAND: i32 = 0x0100;

/// Non-testable events (may not be specified in events field).
pub const POLLERR: i32 = 0x0008;
pub const POLLHUP: i32 = 0x0010;
pub const POLLNVAL: i32 = 0x0020;

/// Infinite timeout value.
pub const INFTIM: i32 = -1;
