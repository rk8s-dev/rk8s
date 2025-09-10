// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/ktrace.h`

use core::mem::size_of;

use crate::{
    c_char, cap_rights_t, pid_t, register_t, sig_t, sigset_t, size_t, timespec_t, timeval_t,
    uio_rw_t, vm_offset_t, MAXCOMLEN,
};

/// operations to ktrace system call  (KTROP(op))
/// set trace points
pub const KTROP_SET: i32 = 0;
/// clear trace points
pub const KTROP_CLEAR: i32 = 1;
/// stop all tracing to file
pub const KTROP_CLEARFILE: i32 = 2;

/// To extract operation
pub const fn KTROP(o: i32) -> i32 {
    o & 3
}

/// flags (ORed in with operation)
/// perform op on all children too
pub const KTRFLAG_DESCEND: i32 = 4;

/// ktrace record header
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ktr_header_v0_t {
    /// length of buf
    pub ktr_len: i32,

    /// trace record type
    pub ktr_type: i16,

    /// process id
    pub ktr_pid: pid_t,

    /// command name
    pub ktr_comm: [c_char; MAXCOMLEN + 1],

    /// timestamp
    pub ktr_time: timeval_t,

    /// thread id
    pub ktr_tid: isize,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ktr_header_t {
    /// length of buf
    pub ktr_len: i32,

    /// trace record type
    pub ktr_type: i16,

    /// ktr_header version
    pub ktr_version: i16,

    /// process id
    pub ktr_pid: pid_t,

    /// command name
    pub ktr_comm: [c_char; MAXCOMLEN + 1],

    /// timestamp
    pub ktr_time: timespec_t,

    /// thread id
    pub ktr_tid: isize,

    /// cpu id
    pub ktr_cpu: i32,
}

pub const KTR_VERSION0: i32 = 0;
pub const KTR_VERSION1: i32 = 1;
pub const KTR_OFFSET_V0: i32 =
    size_of::<ktr_header_v0_t>() as i32 - size_of::<ktr_header_t>() as i32;

/// KTR_SYSCALL - system call record
pub const KTR_SYSCALL: i32 = 1;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ktr_syscall_t {
    /// syscall number
    pub ktr_code: i16,

    /// number of arguments
    pub ktr_narg: i16,

    /// followed by ktr_narg register_t
    pub ktr_args: [register_t; 1],
}

/// KTR_SYSRET - return from system call record
pub const KTR_SYSRET: i32 = 2;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ktr_sysret_t {
    pub ktr_code: i16,
    pub ktr_eosys: i16,
    pub ktr_error: i32,
    pub ktr_retval: register_t,
}

/// KTR_NAMEI - namei record
pub const KTR_NAMEI: i32 = 3;

/// KTR_GENIO - trace generic process i/o
pub const KTR_GENIO: i32 = 4;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ktr_genio_t {
    pub ktr_fd: i32,
    pub ktr_rw: uio_rw_t,
    // followed by data successfully read/written
}

/// KTR_PSIG - trace processed signal
pub const KTR_PSIG: i32 = 5;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ktr_psig_t {
    pub signo: i32,
    pub action: sig_t,
    pub code: i32,
    pub mask: sigset_t,
}

/// KTR_CSW - trace context switches
pub const KTR_CSW: i32 = 6;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ktr_csw_old_t {
    /// 1 if switch out, 0 if switch in
    pub out: i32,

    /// 1 if usermode (ivcsw), 0 if kernel (vcsw)
    pub user: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ktr_csw_t {
    /// 1 if switch out, 0 if switch in
    pub out: i32,

    // 1 if usermode (ivcsw), 0 if kernel (vcsw)
    pub user: i32,

    pub wmesg: [c_char; 8],
}

/// KTR_USER - data coming from userland
///
/// maximum length of passed data
pub const KTR_USER_MAXLEN: i32 = 2048;
pub const KTR_USER: i32 = 7;

/// KTR_STRUCT - misc. structs
pub const KTR_STRUCT: i32 = 8;

/// KTR_SYSCTL - name of a sysctl MIB
///
/// record contains null-terminated MIB name
pub const KTR_SYSCTL: i32 = 9;

/// KTR_PROCCTOR - trace process creation (multiple ABI support)
pub const KTR_PROCCTOR: i32 = 10;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ktr_proc_ctor_t {
    /// struct sysentvec sv_flags copy
    pub sv_flags: u32,
}

/// KTR_PROCDTOR - trace process destruction (multiple ABI support)
pub const KTR_PROCDTOR: i32 = 11;

/// KTR_CAPFAIL - trace capability check failures
pub const KTR_CAPFAIL: i32 = 12;

#[repr(C)]
#[derive(Debug, Clone)]
pub enum ktr_cap_fail_type_t {
    /// insufficient capabilities in cap_check()
    CAPFAIL_NOTCAPABLE,

    /// attempt to increase capabilities
    CAPFAIL_INCREASE,

    /// disallowed system call
    CAPFAIL_SYSCALL,

    /// disallowed VFS lookup
    CAPFAIL_LOOKUP,
}

impl Default for ktr_cap_fail_type_t {
    fn default() -> Self {
        Self::CAPFAIL_NOTCAPABLE
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ktr_cap_fail_t {
    pub cap_type: ktr_cap_fail_type_t,
    pub cap_needed: cap_rights_t,
    pub cap_held: cap_rights_t,
}

/// KTR_FAULT - page fault record
pub const KTR_FAULT: i32 = 13;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ktr_fault_t {
    pub vaddr: vm_offset_t,
    pub type_: i32,
}

/// KTR_FAULTEND - end of page fault record
pub const KTR_FAULTEND: i32 = 14;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ktr_faultend_t {
    pub result: i32,
}

/// KTR_STRUCT_ARRAY - array of misc. structs
pub const KTR_STRUCT_ARRAY: i32 = 15;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ktr_struct_array_t {
    pub struct_size: size_t,
    // Followed by null-terminated structure name and then payload contents.
}

/// KTR_DROP - If this bit is set in ktr_type, then at least one event
/// between the previous record and this record was dropped.
pub const KTR_DROP: i32 = 0x8000;

/// KTR_VERSIONED - If this bit is set in ktr_type, then the kernel
/// exposes the new struct ktr_header (versioned), otherwise the old
/// struct ktr_header_v0 is exposed.
pub const KTR_VERSIONED: i32 = 0x4000;
pub const KTR_TYPE: i32 = KTR_DROP | KTR_VERSIONED;

/// kernel trace points (in p_traceflag)
pub const KTRFAC_MASK: i32 = 0x00ffffff;
pub const KTRFAC_SYSCALL: i32 = 1 << KTR_SYSCALL;
pub const KTRFAC_SYSRET: i32 = 1 << KTR_SYSRET;
pub const KTRFAC_NAMEI: i32 = 1 << KTR_NAMEI;
pub const KTRFAC_GENIO: i32 = 1 << KTR_GENIO;
pub const KTRFAC_PSIG: i32 = 1 << KTR_PSIG;
pub const KTRFAC_CSW: i32 = 1 << KTR_CSW;
pub const KTRFAC_USER: i32 = 1 << KTR_USER;
pub const KTRFAC_STRUCT: i32 = 1 << KTR_STRUCT;
pub const KTRFAC_SYSCTL: i32 = 1 << KTR_SYSCTL;
pub const KTRFAC_PROCCTOR: i32 = 1 << KTR_PROCCTOR;
pub const KTRFAC_PROCDTOR: i32 = 1 << KTR_PROCDTOR;
pub const KTRFAC_CAPFAIL: i32 = 1 << KTR_CAPFAIL;
pub const KTRFAC_FAULT: i32 = 1 << KTR_FAULT;
pub const KTRFAC_FAULTEND: i32 = 1 << KTR_FAULTEND;
pub const KTRFAC_STRUCT_ARRAY: i32 = 1 << KTR_STRUCT_ARRAY;

/// trace flags (also in p_traceflags)
/// root set this trace
#[allow(overflowing_literals)]
pub const KTRFAC_ROOT: i32 = 0x80000000;
/// pass trace flags to children
pub const KTRFAC_INHERIT: i32 = 0x40000000;
/// last event was dropped
pub const KTRFAC_DROP: i32 = 0x20000000;
