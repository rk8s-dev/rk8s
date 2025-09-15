// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/tcp.h`

/// TCP general constants
/// IPv4 (RFC1122, RFC2581)
pub const TCP_MSS_DEFAULT: u32 = 536;
/// IPv6 (tunneled), EDNS0 (RFC3226)
pub const TCP_MSS_DESIRED: u32 = 1220;

/// TCP socket options
/// Turn off Nagle's algorithm.
pub const TCP_NODELAY: i32 = 1;
/// Limit MSS
pub const TCP_MAXSEG: i32 = 2;
/// Never send partially complete segments
pub const TCP_CORK: i32 = 3;
/// Start keeplives after this period
pub const TCP_KEEPIDLE: i32 = 4;
/// Interval between keepalives
pub const TCP_KEEPINTVL: i32 = 5;
/// Number of keepalives before death
pub const TCP_KEEPCNT: i32 = 6;
/// Number of SYN retransmits
pub const TCP_SYNCNT: i32 = 7;
/// Life time of orphaned FIN-WAIT-2 state
pub const TCP_LINGER2: i32 = 8;
/// Wake up listener only when data arrive
pub const TCP_DEFER_ACCEPT: i32 = 9;
/// Bound advertised window
pub const TCP_WINDOW_CLAMP: i32 = 10;
/// Information about this connection.
pub const TCP_INFO: i32 = 11;
/// Block/reenable quick acks
pub const TCP_QUICKACK: i32 = 12;
/// Congestion control algorithm
pub const TCP_CONGESTION: i32 = 13;
/// TCP MD5 Signature (RFC2385)
pub const TCP_MD5SIG: i32 = 14;
/// Use linear timeouts for thin streams
pub const TCP_THIN_LINEAR_TIMEOUTS: i32 = 16;
/// Fast retrans. after 1 dupack
pub const TCP_THIN_DUPACK: i32 = 17;
/// How long for loss retry before timeout
pub const TCP_USER_TIMEOUT: i32 = 18;
/// TCP sock is under repair right now
pub const TCP_REPAIR: i32 = 19;
pub const TCP_REPAIR_QUEUE: i32 = 20;
pub const TCP_QUEUE_SEQ: i32 = 21;
pub const TCP_REPAIR_OPTIONS: i32 = 22;
/// Enable `FastOpen` on listeners
pub const TCP_FASTOPEN: i32 = 23;
pub const TCP_TIMESTAMP: i32 = 24;
/// limit number of unsent bytes in write queue
pub const TCP_NOTSENT_LOWAT: i32 = 25;
/// Get Congestion Control (optional) info
pub const TCP_CC_INFO: i32 = 26;
/// Record SYN headers for new connections
pub const TCP_SAVE_SYN: i32 = 27;
/// Get SYN headers recorded for connection
pub const TCP_SAVED_SYN: i32 = 28;
/// Get/set window parameters
pub const TCP_REPAIR_WINDOW: i32 = 29;
/// Attempt `FastOpen` with connect
pub const TCP_FASTOPEN_CONNECT: i32 = 30;
/// Attach a ULP to a TCP connection
pub const TCP_ULP: i32 = 31;
/// TCP MD5 Signature with extensions
pub const TCP_MD5SIG_EXT: i32 = 32;
/// Set the key for Fast Open (cookie)
pub const TCP_FASTOPEN_KEY: i32 = 33;
/// Enable TFO without a TFO cookie
pub const TCP_FASTOPEN_NO_COOKIE: i32 = 34;
pub const TCP_ZEROCOPY_RECEIVE: i32 = 35;
/// Notify bytes available to read as a cmsg on read
pub const TCP_INQ: i32 = 36;

pub const TCP_CM_INQ: i32 = TCP_INQ;

/// delay outgoing packets by XX usec
pub const TCP_TX_DELAY: i32 = 37;

pub const TCP_REPAIR_ON: i32 = 1;
pub const TCP_REPAIR_OFF: i32 = 0;
/// Turn off without window probes
pub const TCP_REPAIR_OFF_NO_WP: i32 = -1;

/// for `TCP_INFO` socket option
pub const TCPI_OPT_TIMESTAMPS: i32 = 1;
pub const TCPI_OPT_SACK: i32 = 2;
pub const TCPI_OPT_WSCALE: i32 = 4;
/// ECN was negociated at TCP session init
pub const TCPI_OPT_ECN: i32 = 8;
/// we received at least one packet with ECT
pub const TCPI_OPT_ECN_SEEN: i32 = 16;
/// SYN-ACK acked data in SYN sent or rcvd
pub const TCPI_OPT_SYN_DATA: i32 = 32;

/// for `TCP_MD5SIG` socket option
pub const TCP_MD5SIG_MAXKEYLEN: i32 = 80;

/// `tcp_md5sig` extension flags for `TCP_MD5SIG_EXT`
/// address prefix length
pub const TCP_MD5SIG_FLAG_PREFIX: i32 = 0x1;
/// ifindex set
pub const TCP_MD5SIG_FLAG_IFINDEX: i32 = 0x2;
