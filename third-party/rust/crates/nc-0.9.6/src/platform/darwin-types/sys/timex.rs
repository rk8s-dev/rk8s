// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/timex.h`

use crate::timespec_t;

/// NTP API version
pub const NTP_API: i32 = 4;

/// The following defines establish the performance envelope of the
/// kernel discipline loop. Phase or frequency errors greater than
/// NAXPHASE or MAXFREQ are clamped to these maxima. For update intervals
/// less than MINSEC, the loop always operates in PLL mode; while, for
/// update intervals greater than MAXSEC, the loop always operates in FLL
/// mode. Between these two limits the operating mode is selected by the
/// STA_FLL bit in the status word.
///
/// max phase error (ns)
pub const MAXPHASE: usize = 500000000;
/// max freq error (ns/s)
pub const MAXFREQ: usize = 500000;
/// min FLL update interval (s)
pub const MINSEC: i32 = 256;
/// max PLL update interval (s)
pub const MAXSEC: i32 = 2048;
/// nanoseconds in one second
pub const NANOSECOND: i64 = 1000000000;
/// crude ns/s to scaled PPM
pub const SCALE_PPM: i32 = 65536 / 1000;
/// max time constant
pub const MAXTC: i32 = 10;

/// Codes for PPS (pulse-per-second) signals or leap seconds are not used but kept
/// unchanged and commented for future compatibility.
///
/// Control mode codes (timex.modes)
///
/// set time offset
pub const MOD_OFFSET: i32 = 0x0001;
/// set frequency offset
pub const MOD_FREQUENCY: i32 = 0x0002;
/// set maximum time error
pub const MOD_MAXERROR: i32 = 0x0004;
/// set estimated time error
pub const MOD_ESTERROR: i32 = 0x0008;
/// set clock status bits
pub const MOD_STATUS: i32 = 0x0010;
/// set PLL time constant
pub const MOD_TIMECONST: i32 = 0x0020;
/// set PPS maximum averaging time
pub const MOD_PPSMAX: i32 = 0x0040;
/// set TAI offset
pub const MOD_TAI: i32 = 0x0080;
/// select microsecond resolution
pub const MOD_MICRO: i32 = 0x1000;
/// select nanosecond resolution
pub const MOD_NANO: i32 = 0x2000;
/// select clock B
pub const MOD_CLKB: i32 = 0x4000;
/// select clock A
pub const MOD_CLKA: i32 = 0x8000;

/// Status codes (timex.status)
///
/// enable PLL updates (rw)
pub const STA_PLL: i32 = 0x0001;
/// enable PPS freq discipline (rw)
pub const STA_PPSFREQ: i32 = 0x0002;
/// enable PPS time discipline (rw)
pub const STA_PPSTIME: i32 = 0x0004;
/// enable FLL mode (rw)
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

pub const STA_RONLY: i32 = STA_PPSSIGNAL
    | STA_PPSJITTER
    | STA_PPSWANDER
    | STA_PPSERROR
    | STA_CLOCKERR
    | STA_NANO
    | STA_MODE
    | STA_CLK;

pub const STA_SUPPORTED: i32 =
    STA_PLL | STA_FLL | STA_UNSYNC | STA_FREQHOLD | STA_CLOCKERR | STA_NANO | STA_MODE | STA_CLK;

/// Clock states (ntptimeval.time_state)
///
/// no leap second warning
pub const TIME_OK: i32 = 0;
/// insert leap second warning
pub const TIME_INS: i32 = 1;
/// delete leap second warning
pub const TIME_DEL: i32 = 2;
/// leap second in progress
pub const TIME_OOP: i32 = 3;
/// leap second has occurred
pub const TIME_WAIT: i32 = 4;
/// error (see status word)
pub const TIME_ERROR: i32 = 5;

/// NTP user interface -- ntp_gettime - used to read kernel clock values
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ntptimeval_t {
    /// current time (ns) (ro)
    pub time: timespec_t,

    /// maximum error (us) (ro)
    pub maxerror: isize,

    /// estimated error (us) (ro)
    pub esterror: isize,

    /// TAI offset
    pub tai: isize,

    /// time status
    pub time_state: i32,
}

/// NTP daemon interface -- ntp_adjtime -- used to discipline CPU clock
/// oscillator and control/determine status.
///
/// Note: The offset, precision and jitter members are in microseconds if
/// STA_NANO is zero and nanoseconds if not.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct timex_t {
    /// clock mode bits (wo)
    pub modes: u32,

    /// time offset (ns/us) (rw)
    pub offset: isize,
    /// frequency offset (scaled PPM) (rw)
    pub freq: isize,
    /// maximum error (us) (rw)
    pub maxerror: isize,
    /// estimated error (us) (rw)
    pub esterror: isize,
    /// clock status bits (rw)
    pub status: i32,
    /// poll interval (log2 s) (rw)
    pub constant: isize,
    /// clock precision (ns/us) (ro)
    pub precision: isize,
    /// clock frequency tolerance (scaled PPM) (ro)
    pub tolerance: isize,

    /// The following read-only structure members are used by
    /// the PPS signal discipline that is currently not supported.
    /// They are included for compatibility.
    ///
    /// PPS frequency (scaled PPM) (ro)
    pub ppsfreq: isize,
    /// PPS jitter (ns/us) (ro)
    pub jitter: isize,
    /// interval duration (s) (shift) (ro)
    pub shift: i32,
    /// PPS stability (scaled PPM) (ro)
    pub stabil: isize,
    /// jitter limit exceeded (ro)
    pub jitcnt: isize,
    /// calibration intervals (ro)
    pub calcnt: isize,
    /// calibration errors (ro)
    pub errcnt: isize,
    /// stability limit exceeded (ro)
    pub stbcnt: isize,
}
