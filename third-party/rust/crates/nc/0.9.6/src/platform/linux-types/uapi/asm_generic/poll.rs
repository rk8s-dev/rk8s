// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/poll.h`

use crate::poll_t;

/// These are specified by `iBCS2`
pub const POLLIN: i16 = 0x0001;
pub const POLLPRI: i16 = 0x0002;
pub const POLLOUT: i16 = 0x0004;
pub const POLLERR: i16 = 0x0008;
pub const POLLHUP: i16 = 0x0010;
pub const POLLNVAL: i16 = 0x0020;

/// The rest seem to be more-or-less nonstandard. Check them!
pub const POLLRDNORM: i16 = 0x0040;
pub const POLLRDBAND: i16 = 0x0080;
pub const POLLWRNORM: i16 = 0x0100;
pub const POLLWRBAND: i16 = 0x0200;
pub const POLLMSG: i16 = 0x0400;
pub const POLLREMOVE: i16 = 0x1000;
pub const POLLRDHUP: i16 = 0x2000;

/// currently only for epoll
pub const POLLFREE: poll_t = 0x4000;

pub const POLL_BUSY_LOOP: poll_t = 0x8000;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct pollfd_t {
    /// File descriptor
    pub fd: i32,

    /// Requested events.
    ///
    /// Used as input parameter, a bit mask specifying the events
    /// the application is interested in for the file descriptor fd.
    pub events: i16,

    /// Returned events
    ///
    /// Used an output parameter, filled by the kernel with the events
    /// that actually occurred.
    pub revents: i16,
}
