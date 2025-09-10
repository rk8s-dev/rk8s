// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/idtype.h`

/// Using the solaris constants, some of them are not applicable to us
/// Do not re-order the list, or add elements in the middle as this will
/// break the ABI of the system calls using this.  We set a high private
/// maximum so that new values can be added in the future without
/// changing the width of the type.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum idtype_t {
    /// Me/my process group
    P_MYID = -1,

    /// All processes.
    P_ALL,

    /// A process identifier.
    P_PID,

    /// An LWP identifier.
    P_LWPID,

    /// A parent process identifier.
    P_PPID,

    /// A process group identifier.
    P_PGID,

    /// A session identifier.
    P_SID,

    /// A scheduling class identifier.
    P_CID,

    /// A user identifier.
    P_UID,

    /// A group identifier.
    P_GID,

    /// A task identifier.
    P_TASKID,

    /// A project identifier.
    P_PROJID,

    /// A pool identifier.
    P_POOLID,

    /// A zone identifier.
    P_ZONEID,

    /// A (process) contract identifier.
    P_CTID,

    /// CPU identifier.
    P_CPUID,

    /// Processor set identifier.
    P_PSETID,

    _P_MAXIDTYPE = 0x7fffffff,
}
