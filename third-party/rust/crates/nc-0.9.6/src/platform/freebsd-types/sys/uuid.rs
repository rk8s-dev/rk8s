// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/uuid.h`

/// Length of a node address (an IEEE 802 address).
pub const _UUID_NODE_LEN: usize = 6;

/// A DCE 1.1 compatible source representation of UUIDs.
///
/// See also:
///   - http://www.opengroup.org/dce/info/draft-leach-uuids-guids-01.txt
///   - http://www.opengroup.org/onlinepubs/009629399/apdxa.htm
#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct uuid_t {
    pub time_low: u32,
    pub time_mid: u16,
    pub time_hi_and_version: u16,
    pub clock_seq_hi_and_reserved: u8,
    pub clock_seq_low: u8,
    pub node: [u8; _UUID_NODE_LEN],
}
