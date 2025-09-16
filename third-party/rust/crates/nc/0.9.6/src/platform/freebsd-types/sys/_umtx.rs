// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_umtx.h`

use crate::{lwpid_t, timespec_t, uintptr};

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct umtx_t {
    // TODO(Shaohua): Add volatile flag
    /// Owner of the mutex.
    pub u_owner: usize,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct umutex_t {
    // TODO(Shaohua): Add volatile flag
    /// Owner of the mutex
    pub m_owner: lwpid_t,
    /// Flags of the mutex
    pub m_flags: u32,
    /// Priority protect ceiling
    pub m_ceilings: [u32; 2],
    /// Robust linkage
    pub m_rb_lnk: uintptr,
    #[cfg(target_pointer_width = "32")]
    pub m_pad: u32,
    m_spare: [u32; 2],
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ucond_t {
    // TODO(Shaohua): Add volatile flag
    /// Has waiters in kernel
    pub c_has_waiters: u32,
    /// Flags of the condition variable
    pub c_flags: u32,
    /// Clock id
    pub c_clockid: u32,
    // Spare space
    c_spare: [u32; 1],
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct urwlock_t {
    // TODO(Shaohua): Add volatile flag
    pub rw_state: i32,
    pub rw_flags: u32,
    pub rw_blocked_readers: u32,
    rw_blocked_writers: u32,
    pub rw_spare: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct _usem_t {
    // TODO(Shaohua): Add volatile flag
    pub _has_waiters: u32,
    // TODO(Shaohua): Add volatile flag
    pub _count: u32,
    pub _flags: u32,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct _usem2_t {
    /// Waiters flag in high bit.
    // TODO(Shaohua): Add volatile flag
    pub _count: u32,
    pub _flags: u32,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct _umtx_time_t {
    pub _timeout: timespec_t,
    pub _flags: u32,
    pub _clockid: u32,
}
