// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/ipcbuf.h`

use crate::{gid_t, key_t, mode_t, uid_t};

/// The generic `ipc64_perm` structure:
/// Note extra padding because this structure is passed back and forth
/// between kernel and user space.
///
/// `ipc64_perm` was originally meant to be architecture specific, but
/// everyone just ended up making identical copies without specific
/// optimizations, so we may just as well all use the same one.
///
/// Pad space is left for:
/// - 32-bit `mode_t` on architectures that only had 16 bit
/// - 32-bit seq
/// - 2 miscellaneous 32-bit values
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ipc64_perm_t {
    pub key: key_t,
    pub uid: uid_t,
    pub gid: gid_t,
    pub cuid: uid_t,
    pub cgid: gid_t,
    /// pad if `mode_t` is u16:
    pub mode: mode_t,
    pad1: [u8; 4_usize - core::mem::size_of::<mode_t>()],
    pub seq: u16,
    pad2: u16,
    unused1: usize,
    unused2: usize,
}
