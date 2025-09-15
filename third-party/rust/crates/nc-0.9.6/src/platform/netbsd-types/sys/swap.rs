// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/swap.h`

use crate::{c_char, dev_t, PATH_MAX};

/// This structure is used to return swap information for userland
#[repr(C)]
#[derive(Debug, Clone)]
pub struct swapent_t {
    /// device id
    pub se_dev: dev_t,

    /// flags
    pub se_flags: i32,

    /// total blocks
    pub se_nblks: i32,

    /// blocks in use
    pub se_inuse: i32,

    /// priority of this device
    pub se_priority: i32,

    /// path name
    pub se_path: [c_char; PATH_MAX + 1],
}

/// begin swapping on device
pub const SWAP_ON: i32 = 1;
/// stop swapping on device
pub const SWAP_OFF: i32 = 2;
/// how many swap devices ?
pub const SWAP_NSWAP: i32 = 3;
/// old SWAP_STATS, no se_path
pub const SWAP_STATS13: i32 = 4;
/// change priority on device
pub const SWAP_CTL: i32 = 5;
/// old SWAP_STATS, 32 bit dev_t
pub const SWAP_STATS50: i32 = 6;
/// use this device as dump device
pub const SWAP_DUMPDEV: i32 = 7;
/// use this device as dump device
pub const SWAP_GETDUMPDEV: i32 = 8;
/// stop using the dump device
pub const SWAP_DUMPOFF: i32 = 9;
/// get device info
pub const SWAP_STATS: i32 = 10;

/// in use: we have swapped here
pub const SWF_INUSE: i32 = 0x0000_0001;
/// enabled: we can swap here
pub const SWF_ENABLE: i32 = 0x0000_0002;
/// busy: I/O happening here
pub const SWF_BUSY: i32 = 0x0000_0004;
/// fake: still being built
pub const SWF_FAKE: i32 = 0x0000_0008;
