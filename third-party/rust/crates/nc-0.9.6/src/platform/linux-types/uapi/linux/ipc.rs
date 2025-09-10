// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/ipc.h`

#![allow(clippy::module_name_repetitions)]

use crate::{gid_t, key_t, mode_t, msgbuf_t, uid_t};

pub const IPC_PRIVATE: key_t = 0;

/// Obsolete, used only for backwards compatibility and libc5 compiles
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct ipc_perm_t {
    pub key: key_t,
    pub uid: uid_t,
    pub gid: gid_t,
    pub cuid: uid_t,
    pub cgid: gid_t,
    pub mode: mode_t,
    pub seq: u16,
}

/// resource get request flags
/// create if key is nonexistent
pub const IPC_CREAT: i32 = 0o000_1000;
/// fail if key exists
pub const IPC_EXCL: i32 = 0o000_2000;
/// return error on wait
pub const IPC_NOWAIT: i32 = 0o000_4000;

/// these fields are used by the DIPC package so the kernel as standard
/// should avoid using them if possible
///
/// make it distributed
pub const IPC_DIPC: i32 = 0o001_0000;
/// this machine is the DIPC owner
pub const IPC_OWN: i32 = 0o002_0000;

/// Control commands used with semctl, msgctl and shmctl
/// see also specific commands in sem.h, msg.h and shm.h
/// remove resource
pub const IPC_RMID: i32 = 0;
/// Set `ipc_perm` options
pub const IPC_SET: i32 = 1;
/// Get `ipc_perm` options
pub const IPC_STAT: i32 = 2;
/// See ipcs
pub const IPC_INFO: i32 = 3;

/// Version flags for semctl, msgctl, and shmctl commands
/// These are passed as bitflags or-ed with the actual command
pub const IPC_OLD: i32 = 0;
pub const IPC_64: i32 = 0x0100;

/// These are used to wrap system calls.
/// See architecture code for ugly details..
#[repr(C)]
pub struct ipc_kludge_t {
    msgp: *mut msgbuf_t,
    msgtyp: usize,
}

pub const SEMOP: i32 = 1;
pub const SEMGET: i32 = 2;
pub const SEMCTL: i32 = 3;
pub const SEMTIMEDOP: i32 = 4;
pub const MSGSND: i32 = 11;
pub const MSGRCV: i32 = 12;
pub const MSGGET: i32 = 13;
pub const MSGCTL: i32 = 14;
pub const SHMAT: i32 = 21;
pub const SHMDT: i32 = 22;
pub const SHMGET: i32 = 23;
pub const SHMCTL: i32 = 24;

/// Used by the DIPC package, try and avoid reusing it
pub const DIPC: i32 = 25;

// TODO(Shaohua): Remove this macro
//#define IPCCALL(version,op)	((version)<<16 | (op))
