// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/mqueue.h`

pub const MQ_PRIO_MAX: i32 = 32768;
/// per-uid limit of kernel memory used by mqueue, in bytes
pub const MQ_BYTES_MAX: i32 = 819_200;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct mq_attr_t {
    /// message queue flags
    pub mq_flags: isize,
    /// maximum number of messages
    pub mq_maxmsg: isize,
    /// maximum message size
    pub mq_msgsize: isize,
    /// number of messages currently queued
    pub mq_curmsgs: isize,
    /// ignored for input, zeroed for output
    reserved: [isize; 4],
}

/// `SIGEV_THREAD` implementation:
///
/// `SIGEV_THREAD` must be implemented in user space. If `SIGEV_THREAD` is passed
/// to `mq_notify`, then
/// - `sigev_signo` must be the file descriptor of an `AF_NETLINK` socket. It's not
///   necessary that the socket is bound.
/// - `sigev_value.sival_ptr` must point to a cookie that is `NOTIFY_COOKIE_LEN` bytes long.
///
/// If the notification is triggered, then the cookie is sent to the netlink
/// socket. The last byte of the cookie is replaced with the `NOTIFY_??` codes:
/// `NOTIFY_WOKENUP` if the notification got triggered, `NOTIFY_REMOVED` if it was
/// removed, either due to a `close()` on the message queue fd or due to a
/// `mq_notify()` that removed the notification.
pub const NOTIFY_NONE: i32 = 0;
pub const NOTIFY_WOKENUP: i32 = 1;
pub const NOTIFY_REMOVED: i32 = 2;

pub const NOTIFY_COOKIE_LEN: i32 = 32;
