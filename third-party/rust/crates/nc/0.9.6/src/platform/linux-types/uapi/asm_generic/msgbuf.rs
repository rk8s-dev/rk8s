// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/msgbuf.h`

use crate::{ipc64_perm_t, pid_t};

/// Generic `msqid64_ds` structure.
///
/// Note extra padding because this structure is passed back and forth
/// between kernel and user space.
///
/// `msqid64_ds` was originally meant to be architecture specific, but
/// everyone just ended up making identical copies without specific
/// optimizations, so we may just as well all use the same one.
///
/// 64 bit architectures typically define a 64 bit `__kernel_time_t`,
/// so they do not need the first three padding words.
/// On big-endian systems, the padding is in the wrong place.
///
/// Pad space is left for:
/// - 2 miscellaneous 32-bit values
#[cfg(target_pointer_width = "64")]
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct msqid64_ds_t {
    pub msg_perm: ipc64_perm_t,

    /// last msgsnd time
    pub msg_stime: isize,
    /// last msgrcv time
    pub msg_rtime: isize,
    /// last change time
    pub msg_ctime: isize,

    /// current number of bytes on queue
    pub msg_cbytes: usize,
    /// number of messages in queue
    pub msg_qnum: usize,
    /// max number of bytes on queue
    pub msg_qbytes: usize,
    /// pid of last msgsnd
    pub msg_lspid: pid_t,
    /// last receive pid
    pub msg_lrpid: pid_t,
    unused4: usize,
    unused5: usize,
}

#[cfg(target_pointer_width = "32")]
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct msqid64_ds_t {
    pub msg_perm: ipc64_perm_t,

    /// last msgsnd time
    pub msg_stime: usize,
    pub msg_stime_high: usize,
    /// last msgrcv time
    pub msg_rtime: usize,
    pub msg_rtime_high: usize,
    /// last change time
    pub msg_ctime: usize,
    pub msg_ctime_high: usize,

    /// current number of bytes on queue
    pub msg_cbytes: usize,
    /// number of messages in queue
    pub msg_qnum: usize,
    /// max number of bytes on queue
    pub msg_qbytes: usize,
    /// pid of last msgsnd
    pub msg_lspid: pid_t,
    /// last receive pid
    pub msg_lrpid: pid_t,
    unused4: usize,
    unused5: usize,
}
