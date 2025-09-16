// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/umtx.h`

use crate::LONG_MIN;

pub const UMTX_UNOWNED: isize = 0x0;
pub const UMTX_CONTESTED: isize = LONG_MIN;

/// Common lock flags
/// Process shared sync objs
pub const USYNC_PROCESS_SHARED: i32 = 0x0001;

/// umutex flags
/// Priority inherited mutex
pub const UMUTEX_PRIO_INHERIT: i32 = 0x0004;
/// Priority protect mutex
pub const UMUTEX_PRIO_PROTECT: i32 = 0x0008;
/// Robust mutex
pub const UMUTEX_ROBUST: i32 = 0x0010;
/// Robust locked but not consistent
pub const UMUTEX_NONCONSISTENT: i32 = 0x0020;

/// The umutex.m_lock values and bits.  The m_owner is the word which
/// serves as the lock.  Its high bit is the contention indicator and
/// rest of bits records the owner TID.  TIDs values start with PID_MAX
/// + 2 and end by INT32_MAX.  The low range [1..PID_MAX] is guaranteed
/// to be useable as the special markers.
pub const UMUTEX_UNOWNED: u32 = 0x0;
pub const UMUTEX_CONTESTED: u32 = 0x80000000;
pub const UMUTEX_RB_OWNERDEAD: u32 = UMUTEX_CONTESTED | 0x10;
pub const UMUTEX_RB_NOTRECOV: u32 = UMUTEX_CONTESTED | 0x11;

/// urwlock flags
pub const URWLOCK_PREFER_READER: u32 = 0x0002;

pub const URWLOCK_WRITE_OWNER: u32 = 0x80000000;
pub const URWLOCK_WRITE_WAITERS: u32 = 0x40000000;
pub const URWLOCK_READ_WAITERS: u32 = 0x20000000;
pub const URWLOCK_MAX_READERS: u32 = 0x1fffffff;
pub const fn URWLOCK_READER_COUNT(c: u32) -> u32 {
    c & URWLOCK_MAX_READERS
}

/// _usem flags
pub const SEM_NAMED: i32 = 0x0002;

/// _usem2 count field
pub const USEM_HAS_WAITERS: u32 = 0x80000000;
pub const USEM_MAX_COUNT: u32 = 0x7fffffff;
pub const fn USEM_COUNT(c: u32) -> u32 {
    c & USEM_MAX_COUNT
}

/// op code for _umtx_op
/// COMPAT10
pub const UMTX_OP_LOCK: i32 = 0;
/// COMPAT10
pub const UMTX_OP_UNLOCK: i32 = 1;
pub const UMTX_OP_WAIT: i32 = 2;
pub const UMTX_OP_WAKE: i32 = 3;
pub const UMTX_OP_MUTEX_TRYLOCK: i32 = 4;
pub const UMTX_OP_MUTEX_LOCK: i32 = 5;
pub const UMTX_OP_MUTEX_UNLOCK: i32 = 6;
pub const UMTX_OP_SET_CEILING: i32 = 7;
pub const UMTX_OP_CV_WAIT: i32 = 8;
pub const UMTX_OP_CV_SIGNAL: i32 = 9;
pub const UMTX_OP_CV_BROADCAST: i32 = 10;
pub const UMTX_OP_WAIT_UINT: i32 = 11;
pub const UMTX_OP_RW_RDLOCK: i32 = 12;
pub const UMTX_OP_RW_WRLOCK: i32 = 13;
pub const UMTX_OP_RW_UNLOCK: i32 = 14;
pub const UMTX_OP_WAIT_UINT_PRIVATE: i32 = 15;
pub const UMTX_OP_WAKE_PRIVATE: i32 = 16;
pub const UMTX_OP_MUTEX_WAIT: i32 = 17;
/// deprecated
pub const UMTX_OP_MUTEX_WAKE: i32 = 18;
/// deprecated
pub const UMTX_OP_SEM_WAIT: i32 = 19;
/// deprecated
pub const UMTX_OP_SEM_WAKE: i32 = 20;
pub const UMTX_OP_NWAKE_PRIVATE: i32 = 21;
pub const UMTX_OP_MUTEX_WAKE2: i32 = 22;
pub const UMTX_OP_SEM2_WAIT: i32 = 23;
pub const UMTX_OP_SEM2_WAKE: i32 = 24;
pub const UMTX_OP_SHM: i32 = 25;
pub const UMTX_OP_ROBUST_LISTS: i32 = 26;

/*
 * Flags for ops; the double-underbar convention must be maintained for future
 * additions for the sake of libsysdecode.
 */
pub const UMTX_OP__I386: i32 = 0x40000000;
#[allow(overflowing_literals)]
pub const UMTX_OP__32BIT: i32 = 0x80000000;

/// Flags for UMTX_OP_CV_WAIT
pub const CVWAIT_CHECK_UNPARKING: i32 = 0x01;
pub const CVWAIT_ABSTIME: i32 = 0x02;
pub const CVWAIT_CLOCKID: i32 = 0x04;

pub const UMTX_ABSTIME: i32 = 0x01;

pub const UMTX_CHECK_UNPARKING: i32 = CVWAIT_CHECK_UNPARKING;

/// Flags for UMTX_OP_SHM
pub const UMTX_SHM_CREAT: i32 = 0x0001;
pub const UMTX_SHM_LOOKUP: i32 = 0x0002;
pub const UMTX_SHM_DESTROY: i32 = 0x0004;
pub const UMTX_SHM_ALIVE: i32 = 0x0008;
