// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/msg.h`

use crate::{ipc_perm_t, pid_t, time_t};

/// Used for the number of messages in the message queue
pub type msgqnum_t = usize;

/// Used for the number of bytes allowed in a message queue
pub type msglen_t = usize;

/// Possible values for the fifth parameter to msgrcv(), in addition to the
/// IPC_NOWAIT flag, which is permitted.
///
/// No error if big message
pub const MSG_NOERROR: i32 = 0o10_000;

#[derive(Debug, Default, Clone)]
#[repr(C)]
pub struct msqid_ds_t {
    /// msg queue permissions
    pub msg_perm: ipc_perm_t,
    // RESERVED: kernel use only
    msg_first: i32,
    // RESERVED: kernel use only
    msg_last: i32,
    /// # of bytes on the queue
    pub msg_cbytes: msglen_t,
    /// number of msgs on the queue
    pub msg_qnum: msgqnum_t,
    /// max bytes on the queue
    pub msg_qbytes: msglen_t,
    /// pid of last msgsnd()
    pub msg_lspid: pid_t,
    /// pid of last msgrcv()
    pub msg_lrpid: pid_t,
    /// time of last msgsnd()
    pub msg_stime: time_t,
    // RESERVED: DO NOT USE
    msg_pad1: i32,
    /// time of last msgrcv()
    pub msg_rtime: time_t,
    // RESERVED: DO NOT USE
    msg_pad2: i32,
    /// time of last msgctl()
    pub msg_ctime: time_t,
    // RESERVED: DO NOT USE
    msg_pad3: i32,
    // RESERVED: DO NOT USE
    msg_pad4: [i32; 4],
}
