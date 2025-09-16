// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/priority.h`
//! Process priority specifications.

/// Priority classes.
/// Interrupt thread.
pub const PRI_ITHD: u16 = 1;
/// Real time process.
pub const PRI_REALTIME: u16 = 2;
/// Time sharing process.
pub const PRI_TIMESHARE: u16 = 3;
/// Idle process.
pub const PRI_IDLE: u16 = 4;

/// PRI_FIFO is POSIX.1B SCHED_FIFO.
pub const PRI_FIFO_BIT: u16 = 8;
pub const PRI_FIFO: u16 = PRI_FIFO_BIT | PRI_REALTIME;

pub const fn PRI_BASE(P: u16) -> u16 {
    P & !PRI_FIFO_BIT
}
pub const fn PRI_IS_REALTIME(P: u16) -> bool {
    PRI_BASE(P) == PRI_REALTIME
}

pub const fn PRI_NEED_RR(P: u16) -> bool {
    P != PRI_FIFO
}

/// Priorities.  Note that with 64 run queues, differences less than 4 are
/// insignificant.
///
/// Priorities range from 0 to 255, but differences of less then 4 (RQ_PPQ)
/// are insignificant.  Ranges are as follows:
///
/// Interrupt threads:		0 - 15
/// Realtime user threads:	16 - 47
/// Top half kernel threads:	48 - 87
/// Time sharing user threads:	88 - 223
/// Idle user threads:		224 - 255
///
/// XXX If/When the specific interrupt thread and top half thread ranges
/// disappear, a larger range can be used for user processes.
///
/// Highest priority.
pub const PRI_MIN: u16 = 0;
/// Lowest priority.
pub const PRI_MAX: u16 = 255;

pub const PRI_MIN_ITHD: u16 = PRI_MIN;
pub const PRI_MAX_ITHD: u16 = PRI_MIN_REALTIME - 1;

/// Most hardware interrupt threads run at the same priority, but can
/// decay to lower priorities if they run for full time slices.
pub const PI_REALTIME: u16 = PRI_MIN_ITHD + 0;
pub const PI_INTR: u16 = PRI_MIN_ITHD + 4;
pub const PI_AV: u16 = PI_INTR;
pub const PI_NET: u16 = PI_INTR;
pub const PI_DISK: u16 = PI_INTR;
pub const PI_TTY: u16 = PI_INTR;
pub const PI_DULL: u16 = PI_INTR;
pub const PI_SOFT: u16 = PRI_MIN_ITHD + 8;
pub const PI_SOFTCLOCK: u16 = PI_SOFT;
pub const PI_SWI: u16 = PI_SOFT;

pub const PRI_MIN_REALTIME: u16 = 16;
pub const PRI_MAX_REALTIME: u16 = PRI_MIN_KERN - 1;

pub const PRI_MIN_KERN: u16 = 48;
pub const PRI_MAX_KERN: u16 = PRI_MIN_TIMESHARE - 1;

pub const PSWP: u16 = PRI_MIN_KERN + 0;
pub const PVM: u16 = PRI_MIN_KERN + 4;
pub const PINOD: u16 = PRI_MIN_KERN + 8;
pub const PRIBIO: u16 = PRI_MIN_KERN + 12;
pub const PVFS: u16 = PRI_MIN_KERN + 16;
pub const PZERO: u16 = PRI_MIN_KERN + 20;
pub const PSOCK: u16 = PRI_MIN_KERN + 24;
pub const PWAIT: u16 = PRI_MIN_KERN + 28;
pub const PLOCK: u16 = PRI_MIN_KERN + 32;
pub const PPAUSE: u16 = PRI_MIN_KERN + 36;

pub const PRI_MIN_TIMESHARE: u16 = 88;
pub const PRI_MAX_TIMESHARE: u16 = PRI_MIN_IDLE - 1;

pub const PUSER: u16 = PRI_MIN_TIMESHARE;

pub const PRI_MIN_IDLE: u16 = 224;
pub const PRI_MAX_IDLE: u16 = PRI_MAX;
