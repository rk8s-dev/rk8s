// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/msg.h`

use core::ffi::c_void;
use core::ptr;

use crate::{ipc_perm_t, pid_t, time_t};

/// The MSG_NOERROR identifier value, the msqid_ds struct and the msg struct
/// are as defined by the SV API Intel 386 Processor Supplement.
///
/// don't complain about too long msgs
pub const MSG_NOERROR: i32 = 0o10_000;

pub type msglen_t = usize;
pub type msgqnum_t = usize;

/// There seems to be no prefix reserved for this header, so the name
/// "msg" in "struct msg" and the names of all of the nonstandard members
/// are namespace pollution.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct msqid_ds_t {
    /// msg queue permission bits
    pub msg_perm: ipc_perm_t,
    /// first message in the queue
    //__msg_first: *mut msg_t,
    __msg_first: *const c_void,
    /// last message in the queue
    //__msg_last: *mut msg_t,
    __msg_last: *const c_void,
    /// number of bytes in use on the queue
    pub msg_cbytes: msglen_t,
    /// number of msgs in the queue
    pub msg_qnum: msgqnum_t,
    /// max # of bytes on the queue
    pub msg_qbytes: msglen_t,
    /// pid of last msgsnd()
    pub msg_lspid: pid_t,
    /// pid of last msgrcv()
    pub msg_lrpid: pid_t,
    /// time of last msgsnd()
    pub msg_stime: time_t,
    /// time of last msgrcv()
    pub msg_rtime: time_t,
    /// time of last msgctl()
    pub msg_ctime: time_t,
}

impl Default for msqid_ds_t {
    fn default() -> Self {
        Self {
            msg_perm: ipc_perm_t::default(),
            __msg_first: ptr::null(),
            __msg_last: ptr::null(),
            msg_cbytes: 0,
            msg_qnum: 0,
            msg_qbytes: 0,
            msg_lspid: 0,
            msg_lrpid: 0,
            msg_stime: time_t::default(),
            msg_rtime: time_t::default(),
            msg_ctime: time_t::default(),
        }
    }
}
