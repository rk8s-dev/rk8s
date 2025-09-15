// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/poll.h`

/// This file is intended to be compatible with the traditional poll.h.
pub type nfds_t = u32;

/// This structure is passed as an array to poll(2).
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct pollfd_t {
    /// which file descriptor to poll
    pub fd: i32,

    /// events we are interested in
    pub events: i16,

    /// events found on return
    pub revents: i16,
}

/// Requestable events.  If poll(2) finds any of these set, they are
/// copied to revents on return.
/// any readable data available
pub const POLLIN: i32 = 0x0001;
/// OOB/Urgent readable data
pub const POLLPRI: i32 = 0x0002;
/// file descriptor is writeable
pub const POLLOUT: i32 = 0x0004;
/// non-OOB/URG data available
pub const POLLRDNORM: i32 = 0x0040;
/// no write type differentiation
pub const POLLWRNORM: i32 = POLLOUT;
/// OOB/Urgent readable data
pub const POLLRDBAND: i32 = 0x0080;
/// OOB/Urgent data can be written
pub const POLLWRBAND: i32 = 0x0100;

/// General FreeBSD extension (currently only supported for sockets):
/// like POLLIN, except ignore EOF
pub const POLLINIGNEOF: i32 = 0x2000;
/// half shut down
pub const POLLRDHUP: i32 = 0x4000;

/// These events are set if they occur regardless of whether they were requested.
/// some poll error occurred
pub const POLLERR: i32 = 0x0008;
/// file descriptor was "hung up"
pub const POLLHUP: i32 = 0x0010;
/// requested events "invalid"
pub const POLLNVAL: i32 = 0x0020;

pub const POLLSTANDARD: i32 = POLLIN
    | POLLPRI
    | POLLOUT
    | POLLRDNORM
    | POLLRDBAND
    | POLLWRBAND
    | POLLERR
    | POLLHUP
    | POLLNVAL;

/// Request that poll() wait forever.
pub const INFTIM: i32 = -1;
