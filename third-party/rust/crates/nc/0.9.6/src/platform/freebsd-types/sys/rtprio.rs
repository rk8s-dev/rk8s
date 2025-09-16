// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/rtprio.h`
//! Process realtime-priority specifications to rtprio.

use crate::{PRI_FIFO, PRI_FIFO_BIT, PRI_IDLE, PRI_ITHD, PRI_REALTIME, PRI_TIMESHARE};

/// priority types.  Start at 1 to catch uninitialized fields.
/// Interrupt thread.
pub const RTP_PRIO_ITHD: u16 = PRI_ITHD;
/// real time process
pub const RTP_PRIO_REALTIME: u16 = PRI_REALTIME;
/// time sharing process
pub const RTP_PRIO_NORMAL: u16 = PRI_TIMESHARE;
/// idle process
pub const RTP_PRIO_IDLE: u16 = PRI_IDLE;

/// RTP_PRIO_FIFO is POSIX.1B SCHED_FIFO.
pub const RTP_PRIO_FIFO_BIT: u16 = PRI_FIFO_BIT;
pub const RTP_PRIO_FIFO: u16 = PRI_FIFO;

/// priority range
/// Highest priority
pub const RTP_PRIO_MIN: u16 = 0;
/// Lowest priority
pub const RTP_PRIO_MAX: u16 = 31;

/// rtprio() syscall functions
pub const RTP_LOOKUP: i32 = 0;
pub const RTP_SET: i32 = 1;

/// Scheduling class information.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct rtprio_t {
    /// scheduling class
    pub type_: u16,
    pub prio: u16,
}
