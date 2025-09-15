// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/msg.h`

use crate::{ipc_perm_t, ipc_pid_t, time_t};

/// ipcs ctl commands
pub const MSG_STAT: i32 = 11;
pub const MSG_INFO: i32 = 12;
pub const MSG_STAT_ANY: i32 = 13;

/// msgrcv options
/// no error if message is too big
pub const MSG_NOERROR: i32 = 0o10000;
/// recv any msg except of specified type.
pub const MSG_EXCEPT: i32 = 0o20000;
/// copy (not remove) all queue messages
pub const MSG_COPY: i32 = 0o40000;

// Obsolete, used only for backwards compatibility and libc5 compiles
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct msqid_ds_t {
    pub msg_perm: ipc_perm_t,
    /// first message on queue,unused
    pub msg_first: usize,
    /// last message in queue,unused
    pub msg_last: usize,
    /// last msgsnd time
    pub msg_stime: time_t,
    /// last msgrcv time
    pub msg_rtime: time_t,
    /// last change time
    pub msg_ctime: time_t,
    /// Reuse junk fields for 32 bit
    pub msg_lcbytes: usize,
    /// ditto
    pub msg_lqbytes: usize,
    /// current number of bytes on queue
    pub msg_cbytes: u16,
    /// number of messages in queue
    pub msg_qnum: u16,
    /// max number of bytes on queue
    pub msg_qbytes: u16,
    /// pid of last msgsnd
    pub msg_lspid: ipc_pid_t,
    /// last receive pid
    pub msg_lrpid: ipc_pid_t,
}

/// message buffer for msgsnd and msgrcv calls
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct msgbuf_t {
    /// type of message
    pub mtype: isize,
    /// message text
    pub mtext: [u8; 1],
}

/// buffer for msgctl calls `IPC_INFO`, `MSG_INFO`
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct msginfo_t {
    pub msgpool: i32,
    pub msgmap: i32,
    pub msgmax: i32,
    pub msgmnb: i32,
    pub msgmni: i32,
    pub msgssz: i32,
    pub msgtql: i32,
    pub msgseg: u16,
}

/// MSGMNI, MSGMAX and MSGMNB are default values which can be
/// modified by sysctl.
///
/// MSGMNI is the upper limit for the number of messages queues per
/// namespace.
///
/// It has been chosen to be as large possible without facilitating
/// scenarios where userspace causes overflows when adjusting the limits via
/// operations of the form retrieve current limit; add X; update limit".
///
/// MSGMNB is the default size of a new message queue. Non-root tasks can
/// decrease the size with msgctl(`IPC_SET`), root tasks
/// (actually: `CAP_SYS_RESOURCE`) can both increase and decrease the queue
/// size. The optimal value is application dependent.
/// 16384 is used because it was always used (since 0.99.10)
///
/// MAXMAX is the maximum size of an individual message, it's a global
/// (per-namespace) limit that applies for all message queues.
/// It's set to 1/2 of MSGMNB, to ensure that at least two messages fit into
/// the queue. This is also an arbitrary choice (since 2.6.0).
/// <= IPCMNI,  max # of msg queue identifiers
pub const MSGMNI: i32 = 32000;
/// <= `INT_MAX`, max size of message (bytes)
pub const MSGMAX: i32 = 8192;
/// <= `INT_MAX`,  default max size of a message queue
pub const MSGMNB: i32 = 16384;

/// unused
pub const MSGPOOL: i32 = MSGMNI * MSGMNB / 1024; /* size in kbytes of message pool */
/// number of system message headers
pub const MSGTQL: i32 = MSGMNB;
/// number of entries in message map
pub const MSGMAP: i32 = MSGMNB;
/// message segment size
pub const MSGSSZ: i32 = 16;
/// max no. of segments
pub const MSGSEG: i32 = (MSGPOOL * 1024) / MSGSSZ;
