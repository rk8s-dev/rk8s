// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `bsm/audit.h`
#![allow(non_upper_case_globals)]
#![allow(overflowing_literals)]

use crate::{au_token_t, dev_t, pid_t, size_t, time_t, uid_t};

pub const AUDIT_RECORD_MAGIC: i32 = 0x828a0f1b;
pub const MAX_AUDIT_RECORDS: usize = 20;
pub const MAXAUDITDATA: usize = 0x8000 - 1;
pub const MAX_AUDIT_RECORD_SIZE: usize = MAXAUDITDATA;
pub const MIN_AUDIT_FILE_SIZE: usize = 512 * 1024;

pub const AUDIT_HARD_LIMIT_FREE_BLOCKS: i32 = 4;

/// Triggers for the audit daemon.
pub const AUDIT_TRIGGER_MIN: i32 = 1;
/// Below low watermark.
pub const AUDIT_TRIGGER_LOW_SPACE: i32 = 1;
/// Kernel requests rotate.
pub const AUDIT_TRIGGER_ROTATE_KERNEL: i32 = 2;
/// Re-read config file.
pub const AUDIT_TRIGGER_READ_FILE: i32 = 3;
/// Terminate audit.
pub const AUDIT_TRIGGER_CLOSE_AND_DIE: i32 = 4;
/// Below min free space.
pub const AUDIT_TRIGGER_NO_SPACE: i32 = 5;
/// User requests rotate.
pub const AUDIT_TRIGGER_ROTATE_USER: i32 = 6;
/// User initialize of auditd.
pub const AUDIT_TRIGGER_INITIALIZE: i32 = 7;
/// User expiration of trails.
pub const AUDIT_TRIGGER_EXPIRE_TRAILS: i32 = 8;
pub const AUDIT_TRIGGER_MAX: i32 = 8;

/// The special device filename (FreeBSD).
pub const AUDITDEV_FILENAME: &str = "audit";
pub const AUDIT_TRIGGER_FILE: &str = "/dev/audit";

/// Pre-defined audit IDs
pub const AU_DEFAUDITID: uid_t = -1_i32 as uid_t;
pub const AU_DEFAUDITSID: i32 = 0;
pub const AU_ASSIGN_ASID: i32 = -1;

/// IPC types.
///
/// Message IPC id.
pub const AT_IPC_MSG: u8 = 1;
/// Semaphore IPC id.
pub const AT_IPC_SEM: u8 = 2;
/// Shared mem IPC id.
pub const AT_IPC_SHM: u8 = 3;

/// Audit conditions.
pub const AUC_UNSET: i32 = 0;
pub const AUC_AUDITING: i32 = 1;
pub const AUC_NOAUDIT: i32 = 2;
pub const AUC_DISABLED: i32 = -1;

/// auditon(2) commands.
pub const A_OLDGETPOLICY: i32 = 2;
pub const A_OLDSETPOLICY: i32 = 3;
pub const A_GETKMASK: i32 = 4;
pub const A_SETKMASK: i32 = 5;
pub const A_OLDGETQCTRL: i32 = 6;
pub const A_OLDSETQCTRL: i32 = 7;
pub const A_GETCWD: i32 = 8;
pub const A_GETCAR: i32 = 9;
pub const A_GETSTAT: i32 = 12;
pub const A_SETSTAT: i32 = 13;
pub const A_SETUMASK: i32 = 14;
pub const A_SETSMASK: i32 = 15;
pub const A_OLDGETCOND: i32 = 20;
pub const A_OLDSETCOND: i32 = 21;
pub const A_GETCLASS: i32 = 22;
pub const A_SETCLASS: i32 = 23;
pub const A_GETPINFO: i32 = 24;
pub const A_SETPMASK: i32 = 25;
pub const A_SETFSIZE: i32 = 26;
pub const A_GETFSIZE: i32 = 27;
pub const A_GETPINFO_ADDR: i32 = 28;
pub const A_GETKAUDIT: i32 = 29;
pub const A_SETKAUDIT: i32 = 30;
pub const A_SENDTRIGGER: i32 = 31;
pub const A_GETSINFO_ADDR: i32 = 32;
pub const A_GETPOLICY: i32 = 33;
pub const A_SETPOLICY: i32 = 34;
pub const A_GETQCTRL: i32 = 35;
pub const A_SETQCTRL: i32 = 36;
pub const A_GETCOND: i32 = 37;
pub const A_SETCOND: i32 = 38;
pub const A_GETSFLAGS: i32 = 39;
pub const A_SETSFLAGS: i32 = 40;
pub const A_GETCTLMODE: i32 = 41;
pub const A_SETCTLMODE: i32 = 42;
pub const A_GETEXPAFTER: i32 = 43;
pub const A_SETEXPAFTER: i32 = 44;

/// Audit policy controls.
pub const AUDIT_CNT: i32 = 0x0001;
pub const AUDIT_AHLT: i32 = 0x0002;
pub const AUDIT_ARGV: i32 = 0x0004;
pub const AUDIT_ARGE: i32 = 0x0008;
pub const AUDIT_SEQ: i32 = 0x0010;
pub const AUDIT_WINDATA: i32 = 0x0020;
pub const AUDIT_USER: i32 = 0x0040;
pub const AUDIT_GROUP: i32 = 0x0080;
pub const AUDIT_TRAIL: i32 = 0x0100;
pub const AUDIT_PATH: i32 = 0x0200;
pub const AUDIT_SCNT: i32 = 0x0400;
pub const AUDIT_PUBLIC: i32 = 0x0800;
pub const AUDIT_ZONENAME: i32 = 0x1000;
pub const AUDIT_PERZONE: i32 = 0x2000;

/// Default audit queue control parameters.
pub const AQ_HIWATER: i32 = 100;
pub const AQ_MAXHIGH: i32 = 10000;
pub const AQ_LOWATER: i32 = 10;
pub const AQ_BUFSZ: usize = MAXAUDITDATA;
pub const AQ_MAXBUFSZ: usize = 1048576;

/// Default minimum percentage free space on file system.
pub const AU_FS_MINFREE: i32 = 20;

/// Type definitions used indicating the length of variable length addresses
/// in tokens containing addresses, such as header fields.
pub const AU_IPv4: i32 = 4;
pub const AU_IPv6: i32 = 16;

/// Reserved audit class mask indicating which classes are unable to have
/// events added or removed by unentitled processes.
pub const AU_CLASS_MASK_RESERVED: i32 = 0x10000000;

/// Audit control modes
pub const AUDIT_CTLMODE_NORMAL: u8 = 1;
pub const AUDIT_CTLMODE_EXTERNAL: u8 = 2;

/// Audit file expire_after op modes
pub const AUDIT_EXPIRE_OP_AND: u8 = 0;
pub const AUDIT_EXPIRE_OP_OR: u8 = 1;

pub type au_id_t = uid_t;
pub type au_asid_t = pid_t;
pub type au_event_t = u16;
pub type au_emod_t = u16;
pub type au_class_t = u32;
pub type au_asflgs_t = u64;
pub type au_ctlmode_t = u8;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct au_tid_t {
    pub port: dev_t,
    pub machine: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct au_tid_addr_t {
    pub at_port: dev_t,
    pub at_type: u32,
    pub at_addr: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct au_mask_t {
    /// Success bits.
    pub am_success: u32,
    /// Failure bits.
    pub am_failure: u32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct auditinfo_t {
    /// Audit user ID.
    pub ai_auid: au_id_t,

    /// Audit masks.
    pub ai_mask: au_mask_t,

    /// Terminal ID.
    pub ai_termid: au_tid_t,

    /// Audit session ID.
    pub ai_asid: au_asid_t,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct auditinfo_addr_t {
    /// Audit user ID.
    pub ai_auid: au_id_t,

    /// Audit masks.
    pub ai_mask: au_mask_t,

    /// Terminal ID.
    pub ai_termid: au_tid_addr_t,

    /// Audit session ID.
    pub ai_asid: au_asid_t,

    /// Audit session flags.
    pub ai_flags: au_asflgs_t,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct auditpinfo_t {
    /// ID of target process.
    pub ap_pid: pid_t,

    /// Audit user ID.
    pub ap_auid: au_id_t,

    /// Audit masks.
    pub ap_mask: au_mask_t,

    /// Terminal ID.
    pub ap_termid: au_tid_t,

    /// Audit session ID.
    pub ap_asid: au_asid_t,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct auditpinfo_addr_t {
    /// ID of target process.
    pub ap_pid: pid_t,

    /// Audit user ID.
    pub ap_auid: au_id_t,

    /// Audit masks.
    pub ap_mask: au_mask_t,

    /// Terminal ID.
    pub ap_termid: au_tid_addr_t,

    /// Audit session ID.
    pub ap_asid: au_asid_t,

    /// Audit session flags.
    pub ap_flags: au_asflgs_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct au_session_t {
    /// Ptr to full audit info.
    pub as_aia_p: *mut auditinfo_addr_t,

    /// Process Audit Masks.
    pub as_mask: au_mask_t,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct au_expire_after_t {
    /// Age after which trail files should be expired
    pub age: time_t,

    /// Aggregate trail size when files should be expired
    pub size: size_t,

    /// Operator used with the above values to determine when files should be expired
    pub op_type: u8,
}

/// Contents of token_t are opaque outside of libbsm.
pub type token_t = au_token_t;

/// Kernel audit queue control parameters:
/// Default:		Maximum:
/// aq_hiwater:	AQ_HIWATER (100)	AQ_MAXHIGH (10000)
/// aq_lowater:	AQ_LOWATER (10)		<aq_hiwater
/// aq_bufsz:	AQ_BUFSZ (32767)	AQ_MAXBUFSZ (1048576)
/// aq_delay:	20			20000 (not used)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct au_qctrl_t {
    /// Max # of audit recs in queue when threads with new ARs get blocked.
    pub aq_hiwater: i32,

    /// # of audit recs in queue when blocked threads get unblocked.
    pub aq_lowater: i32,

    /// Max size of audit record for audit(2).
    pub aq_bufsz: i32,

    /// Queue delay (not used).
    pub aq_delay: i32,

    /// Minimum filesystem percent free space.
    pub aq_minfree: i32,
}

/// Structure for the audit statistics.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct audit_stat_t {
    pub as_version: u32,
    pub as_numevent: u32,
    pub as_generated: i32,
    pub as_nonattrib: i32,
    pub as_kernel: i32,
    pub as_audit: i32,
    pub as_auditctl: i32,
    pub as_enqueue: i32,
    pub as_written: i32,
    pub as_wblocked: i32,
    pub as_rblocked: i32,
    pub as_dropped: i32,
    pub as_totalsize: i32,
    pub as_memused: u32,
}

/// Structure for the audit file statistics.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct audit_fstat_t {
    pub af_filesz: u64,
    pub af_currsz: u64,
}

/// Audit to event class mapping.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct au_evclass_map_t {
    pub ec_number: au_event_t,
    pub ec_class: au_class_t,
}
