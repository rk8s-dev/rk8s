// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/asm-generic/socket.h`

/// For setsockopt(3)
pub const SOL_SOCKET: i32 = 1;

pub const SO_DEBUG: i32 = 1;
pub const SO_REUSEADDR: i32 = 2;
pub const SO_TYPE: i32 = 3;
pub const SO_ERROR: i32 = 4;
pub const SO_DONTROUTE: i32 = 5;
pub const SO_BROADCAST: i32 = 6;
pub const SO_SNDBUF: i32 = 7;
pub const SO_RCVBUF: i32 = 8;
pub const SO_SNDBUFFORCE: i32 = 32;
pub const SO_RCVBUFFORCE: i32 = 33;
pub const SO_KEEPALIVE: i32 = 9;
pub const SO_OOBINLINE: i32 = 10;
pub const SO_NO_CHECK: i32 = 11;
pub const SO_PRIORITY: i32 = 12;
pub const SO_LINGER: i32 = 13;
pub const SO_BSDCOMPAT: i32 = 14;
pub const SO_REUSEPORT: i32 = 15;

/// powerpc only differs in these
pub const SO_PASSCRED: i32 = 16;
pub const SO_PEERCRED: i32 = 17;
pub const SO_RCVLOWAT: i32 = 18;
pub const SO_SNDLOWAT: i32 = 19;
pub const SO_RCVTIMEO_OLD: i32 = 20;
pub const SO_SNDTIMEO_OLD: i32 = 21;

/// Security levels - as per NRL IPv6 - don't actually do anything
pub const SO_SECURITY_AUTHENTICATION: i32 = 22;
pub const SO_SECURITY_ENCRYPTION_TRANSPORT: i32 = 23;
pub const SO_SECURITY_ENCRYPTION_NETWORK: i32 = 24;

pub const SO_BINDTODEVICE: i32 = 25;

/// Socket filtering
pub const SO_ATTACH_FILTER: i32 = 26;
pub const SO_DETACH_FILTER: i32 = 27;
pub const SO_GET_FILTER: i32 = SO_ATTACH_FILTER;

pub const SO_PEERNAME: i32 = 28;

pub const SO_ACCEPTCONN: i32 = 30;

pub const SO_PEERSEC: i32 = 31;
pub const SO_PASSSEC: i32 = 34;

pub const SO_MARK: i32 = 36;

pub const SO_PROTOCOL: i32 = 38;
pub const SO_DOMAIN: i32 = 39;

pub const SO_RXQ_OVFL: i32 = 40;

pub const SO_WIFI_STATUS: i32 = 41;
pub const SCM_WIFI_STATUS: i32 = SO_WIFI_STATUS;
pub const SO_PEEK_OFF: i32 = 42;

/// Instruct lower device to use last 4-bytes of skb data as FCS
pub const SO_NOFCS: i32 = 43;

pub const SO_LOCK_FILTER: i32 = 44;

pub const SO_SELECT_ERR_QUEUE: i32 = 45;

pub const SO_BUSY_POLL: i32 = 46;

pub const SO_MAX_PACING_RATE: i32 = 47;

pub const SO_BPF_EXTENSIONS: i32 = 48;

pub const SO_INCOMING_CPU: i32 = 49;

pub const SO_ATTACH_BPF: i32 = 50;
pub const SO_DETACH_BPF: i32 = SO_DETACH_FILTER;

pub const SO_ATTACH_REUSEPORT_CBPF: i32 = 51;
pub const SO_ATTACH_REUSEPORT_EBPF: i32 = 52;

pub const SO_CNX_ADVICE: i32 = 53;

pub const SCM_TIMESTAMPING_OPT_STATS: i32 = 54;

pub const SO_MEMINFO: i32 = 55;

pub const SO_INCOMING_NAPI_ID: i32 = 56;

pub const SO_COOKIE: i32 = 57;

pub const SCM_TIMESTAMPING_PKTINFO: i32 = 58;

pub const SO_PEERGROUPS: i32 = 59;

pub const SO_ZEROCOPY: i32 = 60;

pub const SO_TXTIME: i32 = 61;
pub const SCM_TXTIME: i32 = SO_TXTIME;

pub const SO_BINDTOIFINDEX: i32 = 62;

pub const SO_TIMESTAMP_OLD: i32 = 29;
pub const SO_TIMESTAMPNS_OLD: i32 = 35;
pub const SO_TIMESTAMPING_OLD: i32 = 37;

pub const SO_TIMESTAMP_NEW: i32 = 63;
pub const SO_TIMESTAMPNS_NEW: i32 = 64;
pub const SO_TIMESTAMPING_NEW: i32 = 65;

pub const SO_RCVTIMEO_NEW: i32 = 66;
pub const SO_SNDTIMEO_NEW: i32 = 67;

//#if __BITS_PER_LONG == 64 || (defined(__x86_64__) && defined(__ILP32__))
/// on 64-bit and x32, avoid the ?: operator
pub const SO_TIMESTAMP: i32 = SO_TIMESTAMP_OLD;
pub const SO_TIMESTAMPNS: i32 = SO_TIMESTAMPNS_OLD;
pub const SO_TIMESTAMPING: i32 = SO_TIMESTAMPING_OLD;

pub const SO_RCVTIMEO: i32 = SO_RCVTIMEO_OLD;
pub const SO_SNDTIMEO: i32 = SO_SNDTIMEO_OLD;

// TODO(Shaohua): Support 32bits
//#else
//#define SO_TIMESTAMP (sizeof(time_t) == sizeof(__kernel_long_t) ? SO_TIMESTAMP_OLD : SO_TIMESTAMP_NEW)
//#define SO_TIMESTAMPNS (sizeof(time_t) == sizeof(__kernel_long_t) ? SO_TIMESTAMPNS_OLD : SO_TIMESTAMPNS_NEW)
//#define SO_TIMESTAMPING (sizeof(time_t) == sizeof(__kernel_long_t) ? SO_TIMESTAMPING_OLD : SO_TIMESTAMPING_NEW)
//
//#define SO_RCVTIMEO (sizeof(time_t) == sizeof(__kernel_long_t) ? SO_RCVTIMEO_OLD : SO_RCVTIMEO_NEW)
//#define SO_SNDTIMEO (sizeof(time_t) == sizeof(__kernel_long_t) ? SO_SNDTIMEO_OLD : SO_SNDTIMEO_NEW)
//#endif

pub const SCM_TIMESTAMP: i32 = SO_TIMESTAMP;
pub const SCM_TIMESTAMPNS: i32 = SO_TIMESTAMPNS;
pub const SCM_TIMESTAMPING: i32 = SO_TIMESTAMPING;
