// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/eventpoll.h`

#![allow(clippy::uninlined_format_args)]

use crate::{poll_t, O_CLOEXEC};

/// Flags for `epoll_create1()`.
pub const EPOLL_CLOEXEC: i32 = O_CLOEXEC;

/// Valid opcodes to issue to `sys_epoll_ctl()`
pub const EPOLL_CTL_ADD: i32 = 1;
pub const EPOLL_CTL_DEL: i32 = 2;
pub const EPOLL_CTL_MOD: i32 = 3;

/// Epoll event masks
pub const EPOLLIN: poll_t = 0x0000_0001;
pub const EPOLLPRI: poll_t = 0x0000_0002;
pub const EPOLLOUT: poll_t = 0x0000_0004;
pub const EPOLLERR: poll_t = 0x0000_0008;
pub const EPOLLHUP: poll_t = 0x0000_0010;
pub const EPOLLNVAL: poll_t = 0x0000_0020;
pub const EPOLLRDNORM: poll_t = 0x0000_0040;
pub const EPOLLRDBAND: poll_t = 0x0000_0080;
pub const EPOLLWRNORM: poll_t = 0x0000_0100;
pub const EPOLLWRBAND: poll_t = 0x0000_0200;
pub const EPOLLMSG: poll_t = 0x0000_0400;
pub const EPOLLRDHUP: poll_t = 0x0000_2000;

/// Set exclusive wakeup mode for the target file descriptor
pub const EPOLLEXCLUSIVE: poll_t = 1 << 28;

/// Request the handling of system wakeup events so as to prevent system suspends
/// from happening while those events are being processed.
///
/// Assuming neither `EPOLLET` nor `EPOLLONESHOT` is set, system suspends will not be
/// re-allowed until `epoll_wait` is called again after consuming the wakeup
/// event(s).
///
/// Requires `CAP_BLOCK_SUSPEND`
pub const EPOLLWAKEUP: poll_t = 1 << 29;

/// Set the One Shot behaviour for the target file descriptor
pub const EPOLLONESHOT: poll_t = 1 << 30;

/// Set the Edge Triggered behaviour for the target file descriptor
pub const EPOLLET: poll_t = 1 << 31;

/*
 * On x86-64 make the 64bit structure have the same alignment as the
 * 32bit structure. This makes 32bit emulation easier.
 *
 * UML/x86_64 needs the same packing as x86_64
 */
//#ifdef __x86_64__
//#define EPOLL_PACKED __attribute__((packed))
// TODO(Shaohua): pack struct

#[repr(C)]
#[derive(Clone, Copy)]
pub union epoll_data_t {
    pub ptr: usize,
    pub fd: i32,
    pub v_u32: u32,
    pub v_u64: u64,
}

impl core::fmt::Debug for epoll_data_t {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let val: u64 = unsafe { self.v_u64 };
        write!(f, "epoll_data: {}", val)
    }
}

impl Default for epoll_data_t {
    fn default() -> Self {
        Self { ptr: 0 }
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct epoll_event_t {
    pub events: poll_t,
    pub data: epoll_data_t,
}
