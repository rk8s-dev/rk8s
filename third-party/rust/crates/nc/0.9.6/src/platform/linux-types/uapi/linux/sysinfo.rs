// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/sysinfo.h`

use core::mem::size_of;

pub const SI_LOAD_SHIFT: i32 = 16;

#[allow(clippy::module_name_repetitions)]
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sysinfo_t {
    /// Seconds since boot
    pub uptime: isize,
    /// 1, 5, and 15 minute load averages
    pub loads: [usize; 3],
    /// Total usable main memory size
    pub totalram: usize,
    /// Available memory size
    pub freeram: usize,
    /// Amount of shared memory
    pub sharedram: usize,
    /// Memory used by buffers
    pub bufferram: usize,
    /// Total swap space size
    pub totalswap: usize,
    /// swap space still available
    pub freeswap: usize,
    /// Number of current processes
    pub procs: u16,
    /// Explicit padding for m68k
    pad: u16,
    /// Total high memory size
    pub totalhigh: usize,
    /// Available high memory size
    pub freehigh: usize,
    /// Memory unit size in bytes
    pub mem_unit: u32,
    /// Padding: libc5 uses this..
    f: [u8; 20 - 2 * size_of::<usize>() - size_of::<u32>()],
}
