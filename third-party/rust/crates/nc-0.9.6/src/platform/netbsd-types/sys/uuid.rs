// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/uuid.h`

/// Length of a node address (an IEEE 802 address).
pub const _UUID_NODE_LEN: usize = 6;

/// Length of a printed UUID.
pub const _UUID_STR_LEN: usize = 38;

/// A DCE 1.1 compatible source representation of UUIDs.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct uuid_t {
    pub time_low: u32,
    pub time_mid: u16,
    pub time_hi_and_version: u16,
    pub clock_seq_hi_and_reserved: u8,
    pub clock_seq_low: u8,
    pub node: [u8; _UUID_NODE_LEN],
}
