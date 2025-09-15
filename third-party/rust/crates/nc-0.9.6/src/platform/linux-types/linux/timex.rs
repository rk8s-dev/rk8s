// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/timex.h`

/// switch between adjtime/adjtimex modes
const ADJ_ADJTIME: i32 = 0x8000;
/// old-fashioned adjtime
const ADJ_OFFSET_SINGLESHOT: i32 = 0x0001;
/// read-only adjtime
const ADJ_OFFSET_READONLY: i32 = 0x2000;

/// `SHIFT_PLL` is used as a dampening factor to define how much we
/// adjust the frequency correction for a given offset in PLL mode.
/// It also used in dampening the offset correction, to define how
/// much of the current value in `time_offset` we correct for each
/// second. Changing this value changes the stiffness of the ntp
/// adjustment code. A lower value makes it more flexible, reducing
/// NTP convergence time. A higher value makes it stiffer, increasing
/// convergence time, but making the clock more stable.
///
/// In David Mills' nanokernel reference implementation `SHIFT_PLL` is 4.
/// However this seems to increase convergence time much too long.
///
/// `https://lists.ntp.org/pipermail/hackers/2008-January/003487.html`
///
/// In the above mailing list discussion, it seems the value of 4
/// was appropriate for other Unix systems with HZ=100, and that
/// `SHIFT_PLL` should be decreased as HZ increases. However, Linux's
/// clock steering implementation is HZ independent.
///
/// Through experimentation, a `SHIFT_PLL` value of 2 was found to allow
/// for fast convergence (very similar to the `NTPv3` code used prior to
/// v2.6.19), with good clock stability.
///
///
/// `SHIFT_FLL` is used as a dampening factor to define how much we
/// adjust the frequency correction for a given offset in FLL mode.
/// In David Mills' nanokernel reference implementation `SHIFT_FLL` is 2.
///
/// MAXTC establishes the maximum time constant of the PLL.
///
/// PLL frequency factor (shift)
const SHIFT_PLL: i32 = 2;
/// FLL frequency factor (shift)
const SHIFT_FLL: i32 = 2;
/// maximum time constant (shift)
const MAXTC: i32 = 10;

/// `SHIFT_USEC` defines the scaling (shift) of the `time_freq` and
/// `time_tolerance` variables, which represent the current frequency
/// offset and maximum frequency tolerance.
/// frequency offset scale (shift)
pub const SHIFT_USEC: i32 = 16;
//#define PPM_SCALE ((s64)NSEC_PER_USEC << (NTP_SCALE_SHIFT - SHIFT_USEC))
pub const PPM_SCALE_INV_SHIFT: i32 = 19;
//#define PPM_SCALE_INV ((1LL << (PPM_SCALE_INV_SHIFT + NTP_SCALE_SHIFT)) / \
//		       PPM_SCALE + 1)

/// max phase error (ns)
pub const MAXPHASE: isize = 500_000_000;
/// max frequency error (ns/s)
pub const MAXFREQ: i32 = 500_000;
//#define MAXFREQ_SCALED ((s64)MAXFREQ << NTP_SCALE_SHIFT)
/// min interval between updates (s)
pub const MINSEC: i32 = 256;
/// max interval between updates (s)
pub const MAXSEC: i32 = 2048;
//#define NTP_PHASE_LIMIT ((MAXPHASE / NSEC_PER_USEC) << 5) /* beyond max. dispersion */
// Required to safely shift negative values
//#define shift_right(x, s) ({	\
//	__typeof__(x) __x = (x);	\
//	__typeof__(s) __s = (s);	\
//	__x < 0 ? -(-__x >> __s) : __x >> __s;	\
//})

pub const NTP_SCALE_SHIFT: i32 = 32;

// TODO(Shaohua):
//#define NTP_INTERVAL_FREQ  (HZ)
//#define NTP_INTERVAL_LENGTH (NSEC_PER_SEC/NTP_INTERVAL_FREQ)

/// The clock frequency of the i8253/i8254 PIT
pub const PIT_TICK_RATE: usize = 119_3182;
