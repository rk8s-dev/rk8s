// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/poll.h`
//!
//! This file is intended to be compatible with the traditional poll.h.

/// Requestable events.  If poll(2) finds any of these set, they are
/// copied to revents on return.
///
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

/// FreeBSD extensions: polling on a regular file might return one
/// of these events (currently only supported on local filesystems).
///
/// file may have been extended
pub const POLLEXTEND: i32 = 0x0200;
/// file attributes may have changed
pub const POLLATTRIB: i32 = 0x0400;
/// (un)link/rename may have happened
pub const POLLNLINK: i32 = 0x0800;
/// file's contents may have changed
pub const POLLWRITE: i32 = 0x1000;

/// These events are set if they occur regardless of whether they were requested.
///
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

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct pollfd_t {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

pub type nfds_t = u32;
