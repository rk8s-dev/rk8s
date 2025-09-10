// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `netinet/tcp.h`

#![allow(clippy::module_name_repetitions)]

use core::mem::size_of;

use crate::in_port_t;

pub type tcp_seq_t = u32;
/// connection count per rfc1644
pub type tcp_cc_t = u32;

/// for KAME src sync over BSD*'s
pub type tcp6_seq_t = tcp_seq_t;
/// for KAME src sync over BSD*'s
pub type tcp6hdr_t = tcphdr_t;

/// TCP header.
///
/// Per RFC 793, September, 1981.
#[repr(C)]
pub struct tcphdr_t {
    /// source port
    pub th_sport: in_port_t,
    /// destination port
    pub th_dport: in_port_t,
    /// sequence number
    pub th_seq: tcp_seq_t,
    /// acknowledgement number
    pub th_ack: tcp_seq_t,
    //unsigned int    th_x2:4,        /* (unused) */
    #[cfg(target_endian = "little")]
    /// data offset
    pub th_off: u8,

    #[cfg(target_endian = "big")]
    /// data offset
    pub th_off: u8,
    //unsigned int th_x2:4;                    /* (unused) */
    pub th_flags: u8,
    /// window
    pub th_win: u16,
    /// checksum
    pub th_sum: u16,
    /// urgent pointer
    pub th_urp: u16,
}

pub const TH_FIN: i32 = 0x01;
pub const TH_SYN: i32 = 0x02;
pub const TH_RST: i32 = 0x04;
pub const TH_PUSH: i32 = 0x08;
pub const TH_ACK: i32 = 0x10;
pub const TH_URG: i32 = 0x20;
pub const TH_ECE: i32 = 0x40;
pub const TH_CWR: i32 = 0x80;
pub const TH_FLAGS: i32 = TH_FIN | TH_SYN | TH_RST | TH_ACK | TH_URG | TH_ECE | TH_CWR;
pub const TH_ACCEPT: i32 = TH_FIN | TH_SYN | TH_RST | TH_ACK;

pub const TCPOPT_EOL: i32 = 0;
pub const TCPOPT_NOP: i32 = 1;
pub const TCPOPT_MAXSEG: i32 = 2;
pub const TCPOLEN_MAXSEG: i32 = 4;
pub const TCPOPT_WINDOW: i32 = 3;
pub const TCPOLEN_WINDOW: i32 = 3;
/// Experimental
pub const TCPOPT_SACK_PERMITTED: i32 = 4;
pub const TCPOLEN_SACK_PERMITTED: i32 = 2;
/// Experimental
pub const TCPOPT_SACK: i32 = 5;
/// len of sack block
pub const TCPOLEN_SACK: i32 = 8;
pub const TCPOPT_TIMESTAMP: i32 = 8;
pub const TCPOLEN_TIMESTAMP: i32 = 10;
/// appendix A
pub const TCPOLEN_TSTAMP_APPA: i32 = TCPOLEN_TIMESTAMP + 2;
pub const TCPOPT_TSTAMP_HDR: i32 =
    TCPOPT_NOP << 24 | TCPOPT_NOP << 16 | TCPOPT_TIMESTAMP << 8 | TCPOLEN_TIMESTAMP;

/// Absolute maximum TCP options len
pub const MAX_TCPOPTLEN: i32 = 40;

/// CC options: RFC-1644
pub const TCPOPT_CC: i32 = 11;
pub const TCPOPT_CCNEW: i32 = 12;
pub const TCPOPT_CCECHO: i32 = 13;
pub const TCPOLEN_CC: i32 = 6;
pub const TCPOLEN_CC_APPA: i32 = TCPOLEN_CC + 2;
#[must_use]
pub const fn TCPOPT_CC_HDR(ccopt: i32) -> i32 {
    TCPOPT_NOP << 24 | TCPOPT_NOP << 16 | ccopt << 8 | TCPOLEN_CC
}

/// Keyed MD5: RFC 2385
pub const TCPOPT_SIGNATURE: i32 = 19;
pub const TCPOLEN_SIGNATURE: i32 = 18;
pub const TCPOPT_MULTIPATH: i32 = 30;

pub const TCPOPT_FASTOPEN: i32 = 34;
pub const TCPOLEN_FASTOPEN_REQ: i32 = 2;

/// Option definitions
pub const TCPOPT_SACK_PERMIT_HDR: i32 =
    TCPOPT_NOP << 24 | TCPOPT_NOP << 16 | TCPOPT_SACK_PERMITTED << 8 | TCPOLEN_SACK_PERMITTED;
pub const TCPOPT_SACK_HDR: i32 = TCPOPT_NOP << 24 | TCPOPT_NOP << 16 | TCPOPT_SACK << 8;

/// Miscellaneous constants
///
/// Max # SACK blocks stored at sender side
pub const MAX_SACK_BLKS: i32 = 6;

/// A SACK option that specifies n blocks will have a length of `(8*n + 2)` bytes,
/// so the 40 bytes available for TCP options can specify a maximum of 4 blocks.
///
/// MAX # SACKs sent in any segment
pub const TCP_MAX_SACK: i32 = 4;

/// Default maximum segment size for TCP.
///
/// With an IP MTU of 576, this is 536,
/// but 512 is probably more convenient.
///
/// This should be defined as `MIN(512, IP_MSS - sizeof (struct tcpiphdr))`.
pub const TCP_MSS: i32 = 512;

/// `TCP_MINMSS` is defined to be 216 which is fine for the smallest
/// link MTU (256 bytes, SLIP interface) in the Internet.  However it is very unlikely to come across such low MTU interfaces
/// these days (anno dato 2004).
/// Probably it can be set to 512 without ill effects. But we play safe.
/// Setting this to "0" disables the minmss check.
pub const TCP_MINMSS: i32 = 216;

/// Default maximum segment size for TCP6.
///
/// With an IP6 MSS of 1280, this is 1220,
/// but 1024 is probably more convenient. (xxx kazu in doubt)
///
/// This should be defined as `MIN(1024, IP6_MSS - sizeof (struct tcpip6hdr))`
pub const TCP6_MSS: i32 = 1024;

/// largest value for (unscaled) window
pub const TCP_MAXWIN: i32 = 65535;
/// dflt send window for T/TCP client
pub const TTCP_CLIENT_SND_WND: i32 = 4096;

/// maximum window shift
pub const TCP_MAX_WINSHIFT: i32 = 14;

/// max length of header in bytes
pub const TCP_MAXHLEN: usize = 0xf << 2;

/// max space left for options
pub const TCP_MAXOLEN: usize = TCP_MAXHLEN - size_of::<tcphdr_t>();

/// User-settable options (used with setsockopt).
///
/// don't delay send to coalesce packets
pub const TCP_NODELAY: i32 = 0x01;
/// set maximum segment size
pub const TCP_MAXSEG: i32 = 0x02;
/// don't push last block of write
pub const TCP_NOPUSH: i32 = 0x04;
/// don't use TCP options
pub const TCP_NOOPT: i32 = 0x08;
/// idle time used when `SO_KEEPALIVE` is enabled
pub const TCP_KEEPALIVE: i32 = 0x10;
/// connection timeout
pub const TCP_CONNECTIONTIMEOUT: i32 = 0x20;

/// time after which a connection in persist timeout will terminate.
/// see draft-ananth-tcpm-persist-02.txt
pub const PERSIST_TIMEOUT: i32 = 0x40;

/// time after which tcp retransmissions will be stopped and
/// the connection will be dropped
pub const TCP_RXT_CONNDROPTIME: i32 = 0x80;

/// when this option is set, drop a connection after retransmitting the FIN 3 times.
///
/// It will prevent holding too many mbufs in socket buffer queues.
pub const TCP_RXT_FINDROP: i32 = 0x100;

/// interval between keepalives
pub const TCP_KEEPINTVL: i32 = 0x101;
/// number of keepalives before close
pub const TCP_KEEPCNT: i32 = 0x102;
/// always ack every other packet
pub const TCP_SENDMOREACKS: i32 = 0x103;
/// Enable ECN on a connection
pub const TCP_ENABLE_ECN: i32 = 0x104;
/// Enable/Disable TCP Fastopen on this socket
pub const TCP_FASTOPEN: i32 = 0x105;
/// State of TCP connection
pub const TCP_CONNECTION_INFO: i32 = 0x106;

/// Low water mark for TCP unsent data
pub const TCP_NOTSENT_LOWAT: i32 = 0x201;

/// Timestamps enabled
pub const TCPCI_OPT_TIMESTAMPS: i32 = 0x0000_0001;
/// SACK enabled
pub const TCPCI_OPT_SACK: i32 = 0x0000_0002;
/// Window scaling enabled
pub const TCPCI_OPT_WSCALE: i32 = 0x0000_0004;
/// ECN enabled
pub const TCPCI_OPT_ECN: i32 = 0x0000_0008;

pub const TCPCI_FLAG_LOSSRECOVERY: i32 = 0x0000_0001;
pub const TCPCI_FLAG_REORDERING_DETECTED: i32 = 0x0000_0002;
