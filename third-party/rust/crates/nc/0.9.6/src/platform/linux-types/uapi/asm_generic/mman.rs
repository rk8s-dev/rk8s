// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `linux/uapi/asm-generic/mman.h`

/// stack-like segment
pub const MAP_GROWSDOWN: i32 = 0x0100;
/// ETXTBSY
pub const MAP_DENYWRITE: i32 = 0x0800;
/// mark it as an executable
pub const MAP_EXECUTABLE: i32 = 0x1000;
/// pages are locked
pub const MAP_LOCKED: i32 = 0x2000;
/// don't check for reservations
pub const MAP_NORESERVE: i32 = 0x4000;

/// lock all current mappings
pub const MCL_CURRENT: i32 = 1;
/// lock all future mappings
pub const MCL_FUTURE: i32 = 2;
/// lock all pages that are faulted in
pub const MCL_ONFAULT: i32 = 4;
