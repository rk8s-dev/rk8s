// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `netinet/sctp_uio.h

pub type sctp_assoc_t = u32;

pub const SCTP_FUTURE_ASSOC: i32 = 0;
pub const SCTP_CURRENT_ASSOC: i32 = 1;
pub const SCTP_ALL_ASSOC: i32 = 2;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sctp_event_t {
    pub se_assoc_id: sctp_assoc_t,
    pub se_type: u16,
    pub se_on: u8,
}

/// Compatibility to previous define's
pub type sctp_stream_reset_events_t = sctp_stream_reset_event_t;

/// On/Off setup for subscription to events
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sctp_event_subscribe_t {
    pub sctp_data_io_event: u8,
    pub sctp_association_event: u8,
    pub sctp_address_event: u8,
    pub sctp_send_failure_event: u8,
    pub sctp_peer_error_event: u8,
    pub sctp_shutdown_event: u8,
    pub sctp_partial_delivery_event: u8,
    pub sctp_adaptation_layer_event: u8,
    pub sctp_authentication_event: u8,
    pub sctp_sender_dry_event: u8,
    pub sctp_stream_reset_event: u8,
}

/// ancillary data types
pub const SCTP_INIT: i32 = 0x0001;
pub const SCTP_SNDRCV: i32 = 0x0002;
pub const SCTP_EXTRCV: i32 = 0x0003;
pub const SCTP_SNDINFO: i32 = 0x0004;
pub const SCTP_RCVINFO: i32 = 0x0005;
pub const SCTP_NXTINFO: i32 = 0x0006;
pub const SCTP_PRINFO: i32 = 0x0007;
pub const SCTP_AUTHINFO: i32 = 0x0008;
pub const SCTP_DSTADDRV4: i32 = 0x0009;
pub const SCTP_DSTADDRV6: i32 = 0x000a;

/// ancillary data structures
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sctp_initmsg_t {
    pub sinit_num_ostreams: u16,
    pub sinit_max_instreams: u16,
    pub sinit_max_attempts: u16,
    pub sinit_max_init_timeo: u16,
}

pub const SCTP_ALIGN_RESV_PAD: usize = 92;
pub const SCTP_ALIGN_RESV_PAD_SHORT: usize = 76;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct sctp_sndrcvinfo_t {
    pub sinfo_stream: u16,
    pub sinfo_ssn: u16,
    pub sinfo_flags: u16,
    pub sinfo_ppid: u32,
    pub sinfo_context: u32,
    pub sinfo_timetolive: u32,
    pub sinfo_tsn: u32,
    pub sinfo_cumtsn: u32,
    pub sinfo_assoc_id: sctp_assoc_t,
    pub sinfo_keynumber: u16,
    pub sinfo_keynumber_valid: u16,
    __reserve_pad: [u8; SCTP_ALIGN_RESV_PAD],
}

impl Default for sctp_sndrcvinfo_t {
    fn default() -> Self {
        Self {
            sinfo_stream: 0,
            sinfo_ssn: 0,
            sinfo_flags: 0,
            sinfo_ppid: 0,
            sinfo_context: 0,
            sinfo_timetolive: 0,
            sinfo_tsn: 0,
            sinfo_cumtsn: 0,
            sinfo_assoc_id: 0,
            sinfo_keynumber: 0,
            sinfo_keynumber_valid: 0,
            __reserve_pad: [0; SCTP_ALIGN_RESV_PAD],
        }
    }
}

/// Stream reset event - subscribe to SCTP_STREAM_RESET_EVENT
#[repr(C)]
#[derive(Debug, Clone)]
pub struct sctp_stream_reset_event_t {
    pub strreset_type: u16,
    pub strreset_flags: u16,
    pub strreset_length: u32,
    pub strreset_assoc_id: sctp_assoc_t,
    pub strreset_stream_list: *mut u16,
}

impl Default for sctp_stream_reset_event_t {
    fn default() -> Self {
        Self {
            strreset_type: 0,
            strreset_flags: 0,
            strreset_length: 0,
            strreset_assoc_id: 0,
            strreset_stream_list: 0 as *mut u16,
        }
    }
}

/// flags in stream_reset_event (strreset_flags)
pub const SCTP_STREAM_RESET_INCOMING_SSN: i32 = 0x0001;
pub const SCTP_STREAM_RESET_OUTGOING_SSN: i32 = 0x0002;
pub const SCTP_STREAM_RESET_DENIED: i32 = 0x0004;
pub const SCTP_STREAM_RESET_FAILED: i32 = 0x0008;

// NOTE(Shaohua): Remaining types are deleted
