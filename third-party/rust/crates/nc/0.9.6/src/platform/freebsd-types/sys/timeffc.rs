// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/timeffc.h`

use crate::{bintime_t, ffcounter_t};

/// Feed-forward clock estimate
/// Holds time mark as a ffcounter and conversion to bintime based on current
/// timecounter period and offset estimate passed by the synchronization daemon.
/// Provides time of last daemon update, clock status and bound on error.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ffclock_estimate_t {
    /// Time of last estimates update.
    pub update_time: bintime_t,
    /// Counter value at last update.
    pub update_ffcount: ffcounter_t,
    /// Counter value of next leap second.
    pub leapsec_next: ffcounter_t,
    /// Estimate of counter period.
    pub period: u64,
    /// Bound on absolute clock error [ns].
    pub errb_abs: u32,
    /// Bound on counter rate error [ps/s].
    pub errb_rate: u32,
    /// Clock status.
    pub status: u32,
    /// All leap seconds seen so far.
    pub leapsec_total: i16,
    /// Next leap second (in {-1,0,1}).
    pub leapsec: i8,
}

/// Index into the sysclocks array for obtaining the ASCII name of a particular
/// sysclock.
pub const SYSCLOCK_FBCK: i32 = 0;
pub const SYSCLOCK_FFWD: i32 = 1;

/// Parameters of counter characterisation required by feed-forward algorithms.
pub const FFCLOCK_SKM_SCALE: i32 = 1024;

/// Feed-forward clock status
pub const FFCLOCK_STA_UNSYNC: i32 = 1;
pub const FFCLOCK_STA_WARMUP: i32 = 2;
