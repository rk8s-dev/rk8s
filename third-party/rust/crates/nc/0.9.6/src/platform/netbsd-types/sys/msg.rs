// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/msg.h`

use crate::{ipc_perm_sysctl_t, ipc_perm_t, pid_t, size_t, time_t, uintptr_t};

/// don't complain about too long msgs
pub const MSG_NOERROR: i32 = 010000;

pub type msgqnum_t = usize;
pub type msglen_t = size_t;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct msqid_ds_t {
    /// operation permission strucure
    pub msg_perm: ipc_perm_t,

    /// number of messages in the queue
    pub msg_qnum: msgqnum_t,

    /// max # of bytes in the queue
    pub msg_qbytes: msglen_t,

    /// process ID of last msgsend()
    pub msg_lspid: pid_t,
    /// process ID of last msgrcv()
    pub msg_lrpid: pid_t,

    /// time of last msgsend()
    pub msg_stime: time_t,
    /// time of last msgrcv()
    pub msg_rtime: time_t,
    /// time of last change
    pub msg_ctime: time_t,

    /// These members are private and used only in the internal
    /// implementation of this interface.
    /// first message in the queue
    _msg_first: uintptr_t,
    /// last message in the queue
    _msg_last: uintptr_t,
    /// # of bytes currently in queue
    _msg_cbytes: msglen_t,
}

/// Based on the configuration parameters described in an SVR2 (yes, two)
/// config(1m) man page.
///
/// Each message is broken up and stored in segments that are msgssz bytes
/// long.  For efficiency reasons, this should be a power of two.  Also,
/// it doesn't make sense if it is less than 8 or greater than about 256.
/// Consequently, msginit in kern/sysv_msg.c checks that msgssz is a power of
/// two between 8 and 1024 inclusive (and panic's if it isn't).
#[repr(C)]
#[derive(Debug, Clone)]
pub struct msginfo_t {
    /// max chars in a message
    pub msgmax: i32,

    /// max message queue identifiers
    pub msgmni: i32,

    /// max chars in a queue
    pub msgmnb: i32,

    /// max messages in system
    pub msgtql: i32,

    /// size of a message segment
    pub msgssz: i32,

    /// number of message segments
    pub msgseg: i32,
}

/// Warning: 64-bit structure padding is needed here
#[repr(C)]
#[derive(Debug, Clone)]
pub struct msgid_ds_sysctl_t {
    pub msg_perm: ipc_perm_sysctl_t,
    pub msg_qnum: u64,
    pub msg_qbytes: u64,
    pub _msg_cbytes: u64,

    pub msg_lspid: pid_t,
    pub msg_lrpid: pid_t,

    pub msg_stime: time_t,
    pub msg_rtime: time_t,
    pub msg_ctime: time_t,
    pub pad: i32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct msg_sysctl_info_t {
    pub msginfo: msginfo_t,
    pub msgids: [msgid_ds_sysctl_t; 1],
}

/// Each segment must be 2^N long
pub const MSGSSZ: usize = 8;
/// must be less than 32767
pub const MSGSEG: usize = 2048;
pub const MSGMAX: usize = MSGSSZ * MSGSEG;
/// max # of bytes in a queue
pub const MSGMNB: i32 = 2048;
pub const MSGMNI: i32 = 40;
pub const MSGTQL: i32 = 40;

/// Is this msqid_ds locked?
pub const MSG_LOCKED: i32 = 01000;
