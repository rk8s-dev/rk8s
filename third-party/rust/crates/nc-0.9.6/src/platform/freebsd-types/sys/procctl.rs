// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/procctl.h`

#![allow(overflowing_literals)]

use crate::pid_t;

/// MD PROCCTL verbs start at 0x10000000
pub const PROC_PROCCTL_MD_MIN: i32 = 0x10000000;

/// set protected state
pub const PROC_SPROTECT: i32 = 1;
/// reaping enable
pub const PROC_REAP_ACQUIRE: i32 = 2;
/// reaping disable
pub const PROC_REAP_RELEASE: i32 = 3;
/// reaping status
pub const PROC_REAP_STATUS: i32 = 4;
/// get descendants
pub const PROC_REAP_GETPIDS: i32 = 5;
/// kill descendants
pub const PROC_REAP_KILL: i32 = 6;
/// en/dis ptrace and coredumps
pub const PROC_TRACE_CTL: i32 = 7;
/// query tracing status
pub const PROC_TRACE_STATUS: i32 = 8;
/// trap capability errors
pub const PROC_TRAPCAP_CTL: i32 = 9;
/// query trap capability status
pub const PROC_TRAPCAP_STATUS: i32 = 10;
/// set parent death signal
pub const PROC_PDEATHSIG_CTL: i32 = 11;
/// get parent death signal
pub const PROC_PDEATHSIG_STATUS: i32 = 12;
/// en/dis ASLR
pub const PROC_ASLR_CTL: i32 = 13;
/// query ASLR status
pub const PROC_ASLR_STATUS: i32 = 14;
/// en/dis implicit PROT_MAX
pub const PROC_PROTMAX_CTL: i32 = 15;
/// query implicit PROT_MAX status
pub const PROC_PROTMAX_STATUS: i32 = 16;
/// en/dis stack gap on MAP_STACK
pub const PROC_STACKGAP_CTL: i32 = 17;
/// query stack gap
pub const PROC_STACKGAP_STATUS: i32 = 18;
/// disable setuid/setgid
pub const PROC_NO_NEW_PRIVS_CTL: i32 = 19;
/// query suid/sgid disabled status
pub const PROC_NO_NEW_PRIVS_STATUS: i32 = 20;
/// control W^X
pub const PROC_WXMAP_CTL: i32 = 21;
/// query W^X
pub const PROC_WXMAP_STATUS: i32 = 22;

/// Operations for PROC_SPROTECT (passed in integer arg).
pub const fn PPROT_OP(x: i32) -> i32 {
    x & 0xf
}
pub const PPROT_SET: i32 = 1;
pub const PPROT_CLEAR: i32 = 2;

/// Flags for PROC_SPROTECT (ORed in with operation).
pub const fn PPROT_FLAGS(x: i32) -> i32 {
    x & !0xf
}
pub const PPROT_DESCEND: i32 = 0x10;
pub const PPROT_INHERIT: i32 = 0x20;

/// Result of PREAP_STATUS (returned by value).
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct procctl_reaper_status_t {
    pub rs_flags: u32,
    pub rs_children: u32,
    pub rs_descendants: u32,
    pub rs_reaper: pid_t,
    pub rs_pid: pid_t,
    rs_pad0: [u32; 15],
}

/// struct procctl_reaper_status rs_flags
pub const REAPER_STATUS_OWNED: i32 = 0x00000001;
pub const REAPER_STATUS_REALINIT: i32 = 0x00000002;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct procctl_reaper_pidinfo_t {
    pub pi_pid: pid_t,
    pub pi_subtree: pid_t,
    pub pi_flags: u32,
    pi_pad0: [u32; 15],
}

pub const REAPER_PIDINFO_VALID: i32 = 0x00000001;
pub const REAPER_PIDINFO_CHILD: i32 = 0x00000002;
pub const REAPER_PIDINFO_REAPER: i32 = 0x00000004;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct procctl_reaper_pids_t {
    pub rp_count: u32,
    rp_pad0: [u32; 15],
    pub rp_pids: *mut procctl_reaper_pidinfo_t,
}

impl Default for procctl_reaper_pids_t {
    fn default() -> Self {
        Self {
            rp_count: 0,
            rp_pad0: [0; 15],
            rp_pids: 0 as *mut procctl_reaper_pidinfo_t,
        }
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct procctl_reaper_kill_t {
    /// in - signal to send
    pub rk_sig: i32,

    /// in - REAPER_KILL flags
    pub rk_flags: u32,

    /// in - subtree, if REAPER_KILL_SUBTREE
    pub rk_subtree: pid_t,

    /// out - count of processes successfully killed
    pub rk_killed: u32,

    /// out - first failed pid for which error is returned
    pub rk_fpid: pid_t,

    rk_pad0: [u32; 15],
}

pub const REAPER_KILL_CHILDREN: i32 = 0x00000001;
pub const REAPER_KILL_SUBTREE: i32 = 0x00000002;

pub const PROC_TRACE_CTL_ENABLE: i32 = 1;
pub const PROC_TRACE_CTL_DISABLE: i32 = 2;
pub const PROC_TRACE_CTL_DISABLE_EXEC: i32 = 3;

pub const PROC_TRAPCAP_CTL_ENABLE: i32 = 1;
pub const PROC_TRAPCAP_CTL_DISABLE: i32 = 2;

pub const PROC_ASLR_FORCE_ENABLE: i32 = 1;
pub const PROC_ASLR_FORCE_DISABLE: i32 = 2;
pub const PROC_ASLR_NOFORCE: i32 = 3;
pub const PROC_ASLR_ACTIVE: i32 = 0x80000000;

pub const PROC_PROTMAX_FORCE_ENABLE: i32 = 1;
pub const PROC_PROTMAX_FORCE_DISABLE: i32 = 2;
pub const PROC_PROTMAX_NOFORCE: i32 = 3;
pub const PROC_PROTMAX_ACTIVE: i32 = 0x80000000;

pub const PROC_STACKGAP_ENABLE: i32 = 0x0001;
pub const PROC_STACKGAP_DISABLE: i32 = 0x0002;
pub const PROC_STACKGAP_ENABLE_EXEC: i32 = 0x0004;
pub const PROC_STACKGAP_DISABLE_EXEC: i32 = 0x0008;

pub const PROC_NO_NEW_PRIVS_ENABLE: i32 = 1;
pub const PROC_NO_NEW_PRIVS_DISABLE: i32 = 2;

pub const PROC_WX_MAPPINGS_PERMIT: i32 = 0x0001;
pub const PROC_WX_MAPPINGS_DISALLOW_EXEC: i32 = 0x0002;
pub const PROC_WXORX_ENFORCE: i32 = 0x80000000;
