// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `arch/powerpc/include/asm/rtas.h`

use crate::be32_t;

/// Definitions for talking to the RTAS on CHRP machines.

pub const RTAS_UNKNOWN_SERVICE: i32 = -1;
/// Don't instantiate rtas at/above this value
pub const RTAS_INSTANTIATE_MAX: u64 = 1 << 30;

/// Buffer size for ppc_rtas system call.
pub const RTAS_RMOBUF_MAX: i32 = 64 * 1024;

/// RTAS return status codes
pub const RTAS_NOT_SUSPENDABLE: i32 = -9004;

/// RTAS Busy
pub const RTAS_BUSY: i32 = -2;
pub const RTAS_EXTENDED_DELAY_MIN: i32 = 9900;
pub const RTAS_EXTENDED_DELAY_MAX: i32 = 9905;

/// In general to call RTAS use rtas_token("string") to lookup
/// an RTAS token for the given string (e.g. "event-scan").
/// To actually perform the call use
///    ret = rtas_call(token, n_in, n_out, ...)
/// Where n_in is the number of input parameters and
///       n_out is the number of output parameters
///
/// If the "string" is invalid on this system, RTAS_UNKNOWN_SERVICE
/// will be returned as a token.  rtas_call() does look for this
/// token and error out gracefully so rtas_call(rtas_token("str"), ...)
/// may be safely used for one-shot calls to RTAS.

pub type rtas_arg_t = be32_t;

#[repr(C)]
pub struct rtas_args_t {
    pub token: be32_t,
    pub nargs: be32_t,
    pub nret: be32_t,

    pub args: [rtas_arg_t; 16],

    /// Pointer to return values in args[]
    pub rets: *mut rtas_arg_t,
}

/// RTAS event classes
/// set bit 0
#[allow(overflowing_literals)]
pub const RTAS_INTERNAL_ERROR: i32 = 0x80000000;
/// set bit 1
pub const RTAS_EPOW_WARNING: i32 = 0x40000000;
/// set bit 3
pub const RTAS_HOTPLUG_EVENTS: i32 = 0x10000000;
/// set bit 4
pub const RTAS_IO_EVENTS: i32 = 0x08000000;
#[allow(overflowing_literals)]
pub const RTAS_EVENT_SCAN_ALL_EVENTS: i32 = 0xffffffff;

/// RTAS event severity
pub const RTAS_SEVERITY_FATAL: i32 = 0x5;
pub const RTAS_SEVERITY_ERROR: i32 = 0x4;
pub const RTAS_SEVERITY_ERROR_SYNC: i32 = 0x3;
pub const RTAS_SEVERITY_WARNING: i32 = 0x2;
pub const RTAS_SEVERITY_EVENT: i32 = 0x1;
pub const RTAS_SEVERITY_NO_ERROR: i32 = 0x0;

/// RTAS event disposition
pub const RTAS_DISP_FULLY_RECOVERED: i32 = 0x0;
pub const RTAS_DISP_LIMITED_RECOVERY: i32 = 0x1;
pub const RTAS_DISP_NOT_RECOVERED: i32 = 0x2;

/// RTAS event initiator
pub const RTAS_INITIATOR_UNKNOWN: i32 = 0x0;
pub const RTAS_INITIATOR_CPU: i32 = 0x1;
pub const RTAS_INITIATOR_PCI: i32 = 0x2;
pub const RTAS_INITIATOR_ISA: i32 = 0x3;
pub const RTAS_INITIATOR_MEMORY: i32 = 0x4;
pub const RTAS_INITIATOR_POWERMGM: i32 = 0x5;

/// RTAS event target
pub const RTAS_TARGET_UNKNOWN: i32 = 0x0;
pub const RTAS_TARGET_CPU: i32 = 0x1;
pub const RTAS_TARGET_PCI: i32 = 0x2;
pub const RTAS_TARGET_ISA: i32 = 0x3;
pub const RTAS_TARGET_MEMORY: i32 = 0x4;
pub const RTAS_TARGET_POWERMGM: i32 = 0x5;

/// RTAS event type
pub const RTAS_TYPE_RETRY: i32 = 0x01;
pub const RTAS_TYPE_TCE_ERR: i32 = 0x02;
pub const RTAS_TYPE_INTERN_DEV_FAIL: i32 = 0x03;
pub const RTAS_TYPE_TIMEOUT: i32 = 0x04;
pub const RTAS_TYPE_DATA_PARITY: i32 = 0x05;
pub const RTAS_TYPE_ADDR_PARITY: i32 = 0x06;
pub const RTAS_TYPE_CACHE_PARITY: i32 = 0x07;
pub const RTAS_TYPE_ADDR_INVALID: i32 = 0x08;
pub const RTAS_TYPE_ECC_UNCORR: i32 = 0x09;
pub const RTAS_TYPE_ECC_CORR: i32 = 0x0a;
pub const RTAS_TYPE_EPOW: i32 = 0x40;
pub const RTAS_TYPE_PLATFORM: i32 = 0xE0;
pub const RTAS_TYPE_IO: i32 = 0xE1;
pub const RTAS_TYPE_INFO: i32 = 0xE2;
pub const RTAS_TYPE_DEALLOC: i32 = 0xE3;
pub const RTAS_TYPE_DUMP: i32 = 0xE4;
pub const RTAS_TYPE_HOTPLUG: i32 = 0xE5;
pub const RTAS_TYPE_PMGM_POWER_SW_ON: i32 = 0x60;
pub const RTAS_TYPE_PMGM_POWER_SW_OFF: i32 = 0x61;
pub const RTAS_TYPE_PMGM_LID_OPEN: i32 = 0x62;
pub const RTAS_TYPE_PMGM_LID_CLOSE: i32 = 0x63;
pub const RTAS_TYPE_PMGM_SLEEP_BTN: i32 = 0x64;
pub const RTAS_TYPE_PMGM_WAKE_BTN: i32 = 0x65;
pub const RTAS_TYPE_PMGM_BATTERY_WARN: i32 = 0x66;
pub const RTAS_TYPE_PMGM_BATTERY_CRIT: i32 = 0x67;
pub const RTAS_TYPE_PMGM_SWITCH_TO_BAT: i32 = 0x68;
pub const RTAS_TYPE_PMGM_SWITCH_TO_AC: i32 = 0x69;
pub const RTAS_TYPE_PMGM_KBD_OR_MOUSE: i32 = 0x6a;
pub const RTAS_TYPE_PMGM_ENCLOS_OPEN: i32 = 0x6b;
pub const RTAS_TYPE_PMGM_ENCLOS_CLOSED: i32 = 0x6c;
pub const RTAS_TYPE_PMGM_RING_INDICATE: i32 = 0x6d;
pub const RTAS_TYPE_PMGM_LAN_ATTENTION: i32 = 0x6e;
pub const RTAS_TYPE_PMGM_TIME_ALARM: i32 = 0x6f;
pub const RTAS_TYPE_PMGM_CONFIG_CHANGE: i32 = 0x70;
pub const RTAS_TYPE_PMGM_SERVICE_PROC: i32 = 0x71;
/// Platform Resource Reassignment Notification
pub const RTAS_TYPE_PRRN: i32 = 0xA0;

/// RTAS check-exception vector offset
pub const RTAS_VECTOR_EXTERNAL_INTERRUPT: i32 = 0x500;
