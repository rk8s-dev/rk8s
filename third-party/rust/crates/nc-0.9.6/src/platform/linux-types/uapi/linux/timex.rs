// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/timex.h`

use crate::{time64_t, timeval_t};

/// NTP API version
pub const NTP_API: i32 = 4;

/// syscall interface - used (mainly by NTP daemon)
/// to discipline kernel clock oscillator
#[allow(clippy::module_name_repetitions)]
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct timex_t {
    /// mode selector
    pub modes: u32,
    /// time offset (usec)
    pub offset: isize,
    /// frequency offset (scaled ppm)
    pub freq: isize,
    /// maximum error (usec)
    pub maxerror: isize,
    /// estimated error (usec)
    pub esterror: isize,
    /// clock command/status
    pub status: i32,
    /// pll time constant
    pub constant: isize,
    /// clock precision (usec) (read only)
    pub precision: isize,
    /// clock frequency tolerance (ppm) (read only)
    pub tolerance: isize,
    /// (read only, except for `ADJ_SETOFFSET`)
    pub time: timeval_t,
    /// (modified) usecs between clock ticks
    pub tick: isize,

    /// pps frequency (scaled ppm) (ro)
    pub ppsfreq: isize,
    /// pps jitter (us) (ro)
    pub jitter: isize,
    /// interval duration (s) (shift) (ro)
    pub shift: i32,
    /// pps stability (scaled ppm) (ro)
    pub stabil: isize,
    /// jitter limit exceeded (ro)
    pub jitcnt: isize,
    /// calibration intervals (ro)
    pub calcnt: isize,
    /// calibration errors (ro)
    pub errcnt: isize,
    /// stability limit exceeded (ro)
    pub stbcnt: isize,

    /// TAI offset (ro)
    pub tai: i32,

    pad: [i32; 11],
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct kernel_timex_timeval_t {
    pub tv_sec: time64_t,
    pub tv_usec: i64,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct kernel_timex_t {
    /// mode selector
    pub modes: u32,
    /// pad
    pad1: i32,
    /// time offset (usec)
    pub offset: i64,
    /// frequency offset (scaled ppm)
    pub freq: i64,
    /// maximum error (usec)
    pub maxerror: i64,
    /// estimated error (usec)
    pub esterror: i64,
    /// clock command/status
    pub status: i32,
    /// pad
    pad2: i32,
    /// pll time constant
    pub constant: i64,
    /// clock precision (usec) (read only)
    pub precision: i64,
    /// clock frequency tolerance (ppm) (read only)
    pub tolerance: i64,

    /// (read only, except for `ADJ_SETOFFSET`)
    pub time: kernel_timex_timeval_t,
    /// (modified) usecs between clock ticks
    pub tick: i64,

    /// pps frequency (scaled ppm) (ro)
    pub ppsfreq: i64,
    /// pps jitter (us) (ro)
    pub jitter: i64,
    /// interval duration (s) (shift) (ro)
    pub shift: i32,
    /// pad
    pad3: i32,
    /// pps stability (scaled ppm) (ro)
    pub stabil: i64,
    /// jitter limit exceeded (ro)
    pub jitcnt: i64,
    /// calibration intervals (ro)
    pub calcnt: i64,
    /// calibration errors (ro)
    pub errcnt: i64,
    /// stability limit exceeded (ro)
    pub stbcnt: i64,

    /// TAI offset (ro)
    pub tail: i32,

    pad4: [i32; 11],
}

/// Mode codes (timex.mode)
/// time offset
pub const ADJ_OFFSET: i32 = 0x0001;
/// frequency offset
pub const ADJ_FREQUENCY: i32 = 0x0002;
/// maximum time error
pub const ADJ_MAXERROR: i32 = 0x0004;
/// estimated time error
pub const ADJ_ESTERROR: i32 = 0x0008;
/// clock status
pub const ADJ_STATUS: i32 = 0x0010;
/// pll time constant
pub const ADJ_TIMECONST: i32 = 0x0020;
/// set TAI offset
pub const ADJ_TAI: i32 = 0x0080;
/// add 'time' to current time
pub const ADJ_SETOFFSET: i32 = 0x0100;
/// select microsecond resolution
pub const ADJ_MICRO: i32 = 0x1000;
/// select nanosecond resolution
pub const ADJ_NANO: i32 = 0x2000;
/// tick value
pub const ADJ_TICK: i32 = 0x4000;

/// old-fashioned adjtime
pub const ADJ_OFFSET_SINGLESHOT: i32 = 0x8001;
/// read-only adjtime
pub const ADJ_OFFSET_SS_READ: i32 = 0xa001;

/// NTP userland likes the MOD_ prefix better
pub const MOD_OFFSET: i32 = ADJ_OFFSET;
pub const MOD_FREQUENCY: i32 = ADJ_FREQUENCY;
pub const MOD_MAXERROR: i32 = ADJ_MAXERROR;
pub const MOD_ESTERROR: i32 = ADJ_ESTERROR;
pub const MOD_STATUS: i32 = ADJ_STATUS;
pub const MOD_TIMECONST: i32 = ADJ_TIMECONST;
pub const MOD_TAI: i32 = ADJ_TAI;
pub const MOD_MICRO: i32 = ADJ_MICRO;
pub const MOD_NANO: i32 = ADJ_NANO;

/// Status codes (timex.status)
/// enable PLL updates (rw)
pub const STA_PLL: i32 = 0x0001;
/// enable PPS freq discipline (rw)
pub const STA_PPSFREQ: i32 = 0x0002;
/// enable PPS time discipline (rw)
pub const STA_PPSTIME: i32 = 0x0004;
/// select frequency-lock mode (rw)
pub const STA_FLL: i32 = 0x0008;

/// insert leap (rw)
pub const STA_INS: i32 = 0x0010;
/// delete leap (rw)
pub const STA_DEL: i32 = 0x0020;
/// clock unsynchronized (rw)
pub const STA_UNSYNC: i32 = 0x0040;
/// hold frequency (rw)
pub const STA_FREQHOLD: i32 = 0x0080;

/// PPS signal present (ro)
pub const STA_PPSSIGNAL: i32 = 0x0100;
/// PPS signal jitter exceeded (ro)
pub const STA_PPSJITTER: i32 = 0x0200;
/// PPS signal wander exceeded (ro)
pub const STA_PPSWANDER: i32 = 0x0400;
/// PPS signal calibration error (ro)
pub const STA_PPSERROR: i32 = 0x0800;

/// clock hardware fault (ro)
pub const STA_CLOCKERR: i32 = 0x1000;
/// resolution (0 = us, 1 = ns) (ro)
pub const STA_NANO: i32 = 0x2000;
/// mode (0 = PLL, 1 = FLL) (ro)
pub const STA_MODE: i32 = 0x4000;
/// clock source (0 = A, 1 = B) (ro)
pub const STA_CLK: i32 = 0x8000;

/// read-only bits
pub const STA_RONLY: i32 = STA_PPSSIGNAL
    | STA_PPSJITTER
    | STA_PPSWANDER
    | STA_PPSERROR
    | STA_CLOCKERR
    | STA_NANO
    | STA_MODE
    | STA_CLK;

/// Clock states (`time_state`)
/// clock synchronized, no leap second
pub const TIME_OK: i32 = 0;
/// insert leap second
pub const TIME_INS: i32 = 1;
/// delete leap second
pub const TIME_DEL: i32 = 2;
/// leap second in progress
pub const TIME_OOP: i32 = 3;
/// leap second has occurred
pub const TIME_WAIT: i32 = 4;
/// clock not synchronized
pub const TIME_ERROR: i32 = 5;
/// bw compat
pub const TIME_BAD: i32 = TIME_ERROR;
