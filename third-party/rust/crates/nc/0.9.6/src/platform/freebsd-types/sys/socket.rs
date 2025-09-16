// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/socket.h`

use crate::{sa_family_t, socklen_t, ssize_t};

/// stream socket
pub const SOCK_STREAM: i32 = 1;
/// datagram socket
pub const SOCK_DGRAM: i32 = 2;
/// raw-protocol interface
pub const SOCK_RAW: i32 = 3;
/// reliably-delivered message
pub const SOCK_RDM: i32 = 4;
/// sequenced packet stream
pub const SOCK_SEQPACKET: i32 = 5;

/// Creation flags, OR'ed into `socket()` and `socketpair()` type argument.
pub const SOCK_CLOEXEC: i32 = 0x1000_0000;
pub const SOCK_NONBLOCK: i32 = 0x2000_0000;

/// Flags for `accept1()`, `kern_accept4()` and `solisten_dequeue`, in addition
/// to `SOCK_CLOEXEC` and `SOCK_NONBLOCK`.
pub const ACCEPT4_INHERIT: i32 = 0x1;
pub const ACCEPT4_COMPAT: i32 = 0x2;

/// Option flags per-socket.
/// turn on debugging info recording
pub const SO_DEBUG: i32 = 0x0000_0001;
/// socket has had listen()
pub const SO_ACCEPTCONN: i32 = 0x0000_0002;
/// allow local address reuse
pub const SO_REUSEADDR: i32 = 0x0000_0004;
/// keep connections alive
pub const SO_KEEPALIVE: i32 = 0x0000_0008;
/// just use interface addresses
pub const SO_DONTROUTE: i32 = 0x0000_0010;
/// permit sending of broadcast msgs
pub const SO_BROADCAST: i32 = 0x0000_0020;
/// bypass hardware when possible
pub const SO_USELOOPBACK: i32 = 0x0000_0040;
/// linger on close if data present
pub const SO_LINGER: i32 = 0x0000_0080;
/// leave received OOB data in line
pub const SO_OOBINLINE: i32 = 0x0000_0100;
/// allow local address & port reuse
pub const SO_REUSEPORT: i32 = 0x0000_0200;
/// timestamp received dgram traffic
pub const SO_TIMESTAMP: i32 = 0x0000_0400;
/// no SIGPIPE from EPIPE
pub const SO_NOSIGPIPE: i32 = 0x0000_0800;
/// there is an accept filter
pub const SO_ACCEPTFILTER: i32 = 0x0000_1000;
/// timestamp received dgram traffic
pub const SO_BINTIME: i32 = 0x0000_2000;
/// socket cannot be offloaded
pub const SO_NO_OFFLOAD: i32 = 0x0000_4000;
/// disable direct data placement
pub const SO_NO_DDP: i32 = 0x0000_8000;
/// reuse with load balancing
pub const SO_REUSEPORT_LB: i32 = 0x0001_0000;
/// keep track of receive errors
pub const SO_RERROR: i32 = 0x0002_0000;

/// Additional options, not kept in `so_options`.
/// send buffer size
pub const SO_SNDBUF: i32 = 0x1001;
/// receive buffer size
pub const SO_RCVBUF: i32 = 0x1002;
/// send low-water mark
pub const SO_SNDLOWAT: i32 = 0x1003;
/// receive low-water mark
pub const SO_RCVLOWAT: i32 = 0x1004;
/// send timeout
pub const SO_SNDTIMEO: i32 = 0x1005;
/// receive timeout
pub const SO_RCVTIMEO: i32 = 0x1006;
/// get error status and clear
pub const SO_ERROR: i32 = 0x1007;
/// get socket type
pub const SO_TYPE: i32 = 0x1008;
/// socket's MAC label
pub const SO_LABEL: i32 = 0x1009;
/// socket's peer's MAC label
pub const SO_PEERLABEL: i32 = 0x1010;
/// socket's backlog limit
pub const SO_LISTENQLIMIT: i32 = 0x1011;
/// socket's complete queue length
pub const SO_LISTENQLEN: i32 = 0x1012;
/// socket's incomplete queue length
pub const SO_LISTENINCQLEN: i32 = 0x1013;
/// use this FIB to route
pub const SO_SETFIB: i32 = 0x1014;
/// user cookie (dummynet etc.)
pub const SO_USER_COOKIE: i32 = 0x1015;
/// get socket protocol (Linux name)
pub const SO_PROTOCOL: i32 = 0x1016;
/// alias for `SO_PROTOCOL` (`SunOS` name)
pub const SO_PROTOTYPE: i32 = SO_PROTOCOL;
/// clock type used for `SO_TIMESTAMP`
pub const SO_TS_CLOCK: i32 = 0x1017;
/// socket's max TX pacing rate (Linux name)
pub const SO_MAX_PACING_RATE: i32 = 0x1018;
/// get socket domain
pub const SO_DOMAIN: i32 = 0x1019;

/// microsecond resolution, realtime
pub const SO_TS_REALTIME_MICRO: i32 = 0;
/// sub-nanosecond resolution, realtime
pub const SO_TS_BINTIME: i32 = 1;
/// nanosecond resolution, realtime
pub const SO_TS_REALTIME: i32 = 2;
/// nanosecond resolution, monotonic
pub const SO_TS_MONOTONIC: i32 = 3;
pub const SO_TS_DEFAULT: i32 = SO_TS_REALTIME_MICRO;
pub const SO_TS_CLOCK_MAX: i32 = SO_TS_MONOTONIC;

/// Space reserved for new socket options added by third-party vendors.
/// This range applies to all socket option levels.  New socket options
/// in FreeBSD should always use an option value less than `SO_VENDOR`.
#[allow(overflowing_literals)]
pub const SO_VENDOR: i32 = 0x8000_0000;

/// Level number for (get/set)sockopt() to apply to socket itself.
///
/// options for socket level
pub const SOL_SOCKET: i32 = 0xffff;

/// Address families.
/// unspecified
pub const AF_UNSPEC: i32 = 0;
/// local to host (pipes, portals)
pub const AF_LOCAL: i32 = AF_UNIX;
/// standardized name for `AF_LOCAL`
pub const AF_UNIX: i32 = 1;
/// internetwork: UDP, TCP, etc.
pub const AF_INET: i32 = 2;
/// arpanet imp addresses
pub const AF_IMPLINK: i32 = 3;
/// pup protocols: e.g. BSP
pub const AF_PUP: i32 = 4;
/// mit CHAOS protocols
pub const AF_CHAOS: i32 = 5;
/// SMB protocols
pub const AF_NETBIOS: i32 = 6;
/// ISO protocols
pub const AF_ISO: i32 = 7;
pub const AF_OSI: i32 = AF_ISO;
/// European computer manufacturers
pub const AF_ECMA: i32 = 8;
/// datakit protocols
pub const AF_DATAKIT: i32 = 9;
/// CCITT protocols, X.25 etc
pub const AF_CCITT: i32 = 10;
/// IBM SNA
pub const AF_SNA: i32 = 11;
/// `DECnet`
pub const AF_DECNET: i32 = 12;
/// DEC Direct data link interface
pub const AF_DLI: i32 = 13;
/// LAT
pub const AF_LAT: i32 = 14;
/// NSC Hyperchannel
pub const AF_HYLINK: i32 = 15;
/// Apple Talk
pub const AF_APPLETALK: i32 = 16;
/// Internal Routing Protocol
pub const AF_ROUTE: i32 = 17;
/// Link layer interface
pub const AF_LINK: i32 = 18;
/// eXpress Transfer Protocol (no AF)
pub const PSEUDO_AF_XTP: i32 = 19;
/// connection-oriented IP, aka ST II
pub const AF_COIP: i32 = 20;
/// Computer Network Technology
pub const AF_CNT: i32 = 21;
/// Help Identify RTIP packets
pub const PSEUDO_AF_RTIP: i32 = 22;
/// Novell Internet Protocol
pub const AF_IPX: i32 = 23;
/// Simple Internet Protocol
pub const AF_SIP: i32 = 24;
/// Help Identify PIP packets
pub const PSEUDO_AF_PIP: i32 = 25;
/// Integrated Services Digital Network
pub const AF_ISDN: i32 = 26;
/// CCITT E.164 recommendation
pub const AF_E164: i32 = AF_ISDN;
/// Internal key-management function
pub const PSEUDO_AF_KEY: i32 = 27;
/// IPv6
pub const AF_INET6: i32 = 28;
/// native ATM access
pub const AF_NATM: i32 = 29;
/// ATM
pub const AF_ATM: i32 = 30;
/// Used by BPF to not rewrite headers in interface output routine
pub const PSEUDO_AF_HDRCMPLT: i32 = 31;
/// Netgraph sockets
pub const AF_NETGRAPH: i32 = 32;
/// 802.3ad slow protocol
pub const AF_SLOW: i32 = 33;
/// Sitara cluster protocol
pub const AF_SCLUSTER: i32 = 34;
pub const AF_ARP: i32 = 35;
/// Bluetooth sockets
pub const AF_BLUETOOTH: i32 = 36;
/// IEEE 802.11 protocol
pub const AF_IEEE80211: i32 = 37;
/// OFED Socket Direct Protocol ipv4
pub const AF_INET_SDP: i32 = 40;
/// OFED Socket Direct Protocol ipv6
pub const AF_INET6_SDP: i32 = 42;
/// `HyperV` sockets
pub const AF_HYPERV: i32 = 43;
pub const AF_MAX: i32 = 43;
/// When allocating a new AF_ constant, please only allocate
/// even numbered constants for FreeBSD until 134 as odd numbered AF_
/// constants 39-133 are now reserved for vendors.
pub const AF_VENDOR00: i32 = 39;
pub const AF_VENDOR01: i32 = 41;
pub const AF_VENDOR03: i32 = 45;
pub const AF_VENDOR04: i32 = 47;
pub const AF_VENDOR05: i32 = 49;
pub const AF_VENDOR06: i32 = 51;
pub const AF_VENDOR07: i32 = 53;
pub const AF_VENDOR08: i32 = 55;
pub const AF_VENDOR09: i32 = 57;
pub const AF_VENDOR10: i32 = 59;
pub const AF_VENDOR11: i32 = 61;
pub const AF_VENDOR12: i32 = 63;
pub const AF_VENDOR13: i32 = 65;
pub const AF_VENDOR14: i32 = 67;
pub const AF_VENDOR15: i32 = 69;
pub const AF_VENDOR16: i32 = 71;
pub const AF_VENDOR17: i32 = 73;
pub const AF_VENDOR18: i32 = 75;
pub const AF_VENDOR19: i32 = 77;
pub const AF_VENDOR20: i32 = 79;
pub const AF_VENDOR21: i32 = 81;
pub const AF_VENDOR22: i32 = 83;
pub const AF_VENDOR23: i32 = 85;
pub const AF_VENDOR24: i32 = 87;
pub const AF_VENDOR25: i32 = 89;
pub const AF_VENDOR26: i32 = 91;
pub const AF_VENDOR27: i32 = 93;
pub const AF_VENDOR28: i32 = 95;
pub const AF_VENDOR29: i32 = 97;
pub const AF_VENDOR30: i32 = 99;
pub const AF_VENDOR31: i32 = 101;
pub const AF_VENDOR32: i32 = 103;
pub const AF_VENDOR33: i32 = 105;
pub const AF_VENDOR34: i32 = 107;
pub const AF_VENDOR35: i32 = 109;
pub const AF_VENDOR36: i32 = 111;
pub const AF_VENDOR37: i32 = 113;
pub const AF_VENDOR38: i32 = 115;
pub const AF_VENDOR39: i32 = 117;
pub const AF_VENDOR40: i32 = 119;
pub const AF_VENDOR41: i32 = 121;
pub const AF_VENDOR42: i32 = 123;
pub const AF_VENDOR43: i32 = 125;
pub const AF_VENDOR44: i32 = 127;
pub const AF_VENDOR45: i32 = 129;
pub const AF_VENDOR46: i32 = 131;
pub const AF_VENDOR47: i32 = 133;

/// Structure used by kernel to store most addresses.
#[repr(C)]
#[derive(Debug, Default)]
pub struct sockaddr_t {
    /// total length
    pub sa_len: u8,

    /// address family
    pub sa_family: sa_family_t,

    /// actually longer; address value
    pub sa_data: [u8; 14],
}

/// longest possible addresses
pub const SOCK_MAXADDRLEN: i32 = 255;

/// Structure used by kernel to pass protocol information in raw sockets.
pub struct sockproto_t {
    /// address family
    pub sp_family: u16,

    /// protocol
    pub sp_protocol: u16,
}

/// Protocol families, same as address families for now.
pub const PF_UNSPEC: i32 = AF_UNSPEC;
pub const PF_LOCAL: i32 = AF_LOCAL;
/// backward compatibility
pub const PF_UNIX: i32 = PF_LOCAL;
pub const PF_INET: i32 = AF_INET;
pub const PF_IMPLINK: i32 = AF_IMPLINK;
pub const PF_PUP: i32 = AF_PUP;
pub const PF_CHAOS: i32 = AF_CHAOS;
pub const PF_NETBIOS: i32 = AF_NETBIOS;
pub const PF_ISO: i32 = AF_ISO;
pub const PF_OSI: i32 = AF_ISO;
pub const PF_ECMA: i32 = AF_ECMA;
pub const PF_DATAKIT: i32 = AF_DATAKIT;
pub const PF_CCITT: i32 = AF_CCITT;
pub const PF_SNA: i32 = AF_SNA;
pub const PF_DECNET: i32 = AF_DECNET;
pub const PF_DLI: i32 = AF_DLI;
pub const PF_LAT: i32 = AF_LAT;
pub const PF_HYLINK: i32 = AF_HYLINK;
pub const PF_APPLETALK: i32 = AF_APPLETALK;
pub const PF_ROUTE: i32 = AF_ROUTE;
pub const PF_LINK: i32 = AF_LINK;
/// really just proto family, no AF
pub const PF_XTP: i32 = PSEUDO_AF_XTP;
pub const PF_COIP: i32 = AF_COIP;
pub const PF_CNT: i32 = AF_CNT;
pub const PF_SIP: i32 = AF_SIP;
pub const PF_IPX: i32 = AF_IPX;
/// same format as `AF_INET`
pub const PF_RTIP: i32 = PSEUDO_AF_RTIP;
pub const PF_PIP: i32 = PSEUDO_AF_PIP;
pub const PF_ISDN: i32 = AF_ISDN;
pub const PF_KEY: i32 = PSEUDO_AF_KEY;
pub const PF_INET6: i32 = AF_INET6;
pub const PF_NATM: i32 = AF_NATM;
pub const PF_ATM: i32 = AF_ATM;
pub const PF_NETGRAPH: i32 = AF_NETGRAPH;
pub const PF_SLOW: i32 = AF_SLOW;
pub const PF_SCLUSTER: i32 = AF_SCLUSTER;
pub const PF_ARP: i32 = AF_ARP;
pub const PF_BLUETOOTH: i32 = AF_BLUETOOTH;
pub const PF_IEEE80211: i32 = AF_IEEE80211;
pub const PF_INET_SDP: i32 = AF_INET_SDP;
pub const PF_INET6_SDP: i32 = AF_INET6_SDP;

pub const PF_MAX: i32 = AF_MAX;

/// Definitions for network related sysctl, `CTL_NET`.
///
/// Second level is protocol family.
///
/// Third level is protocol number.
///
/// Further levels are defined by the individual families.
///
/// `PF_ROUTE` - Routing table
///
/// Three additional levels are defined:
/// Fourth: address family, 0 is wildcard
/// Fifth: type of info, defined below
/// Sixth: flag(s) to mask with for `NET_RT_FLAGS`
/// dump; may limit to a.f.
pub const NET_RT_DUMP: i32 = 1;
/// by flags, e.g. RESOLVING
pub const NET_RT_FLAGS: i32 = 2;
/// survey interface list
pub const NET_RT_IFLIST: i32 = 3;
/// return multicast address list
pub const NET_RT_IFMALIST: i32 = 4;
pub const NET_RT_IFLISTL: i32 = 5; /* Survey interface list, using 'l'en
                                    * versions of msghdr structs. */
/// dump routing nexthops
pub const NET_RT_NHOP: i32 = 6;
/// dump routing nexthop groups
pub const NET_RT_NHGRP: i32 = 7;

/// Maximum queue length specifiable by listen.
pub const SOMAXCONN: i32 = 128;

/// Message header for recvmsg and sendmsg calls.
/// Used value-result for recvmsg, value only for sendmsg.
#[repr(C)]
#[derive(Debug, Default)]
pub struct msghdr_t {
    /// optional address
    pub msg_name: usize,

    /// size of address
    pub msg_namelen: socklen_t,

    /// scatter/gather array
    ///
    /// *mut iovec_t
    pub msg_iov: usize,

    /// # elements in msg_iov
    pub msg_iovlen: i32,

    /// ancillary data, see below
    pub msg_control: usize,

    /// ancillary data buffer len
    pub msg_controllen: socklen_t,

    /// flags on received message
    pub msg_flags: i32,
}

/// process out-of-band data
pub const MSG_OOB: i32 = 0x0000_0001;
/// peek at incoming message
pub const MSG_PEEK: i32 = 0x0000_0002;
/// send without using routing tables
pub const MSG_DONTROUTE: i32 = 0x0000_0004;
/// data completes record
pub const MSG_EOR: i32 = 0x0000_0008;
/// data discarded before delivery
pub const MSG_TRUNC: i32 = 0x0000_0010;
/// control data lost before delivery
pub const MSG_CTRUNC: i32 = 0x0000_0020;
/// wait for full request or error
pub const MSG_WAITALL: i32 = 0x0000_0040;
/// this message should be nonblocking
pub const MSG_DONTWAIT: i32 = 0x0000_0080;
/// data completes connection
pub const MSG_EOF: i32 = 0x0000_0100;
// 0x00000200	   unused
// 0x00000400	   unused
// 0x00000800	   unused
// 0x00001000	   unused
/// SCTP notification
pub const MSG_NOTIFICATION: i32 = 0x0000_2000;
/// FIONBIO mode, used by fifofs
pub const MSG_NBIO: i32 = 0x0000_4000;
/// used in sendit()
pub const MSG_COMPAT: i32 = 0x0000_8000;
/// for use by socket callbacks - soreceive (TCP)
pub const MSG_SOCALLBCK: i32 = 0x0001_0000;
/// do not generate SIGPIPE on EOF
pub const MSG_NOSIGNAL: i32 = 0x0002_0000;
/// make received fds close-on-exec
pub const MSG_CMSG_CLOEXEC: i32 = 0x0004_0000;
/// for recvmmsg()
pub const MSG_WAITFORONE: i32 = 0x0008_0000;
/// additional data pending
pub const MSG_MORETOCOME: i32 = 0x0010_0000;
/// only soreceive() app. data (TLS)
pub const MSG_TLSAPPDATA: i32 = 0x0020_0000;

/// "Socket"-level control message types:
/// access rights (array of int)
pub const SCM_RIGHTS: i32 = 0x01;
/// timestamp (struct timeval)
pub const SCM_TIMESTAMP: i32 = 0x02;
/// process creds (struct cmsgcred)
pub const SCM_CREDS: i32 = 0x03;
/// timestamp (struct bintime)
pub const SCM_BINTIME: i32 = 0x04;
/// timestamp (struct timespec)
pub const SCM_REALTIME: i32 = 0x05;
/// timestamp (struct timespec)
pub const SCM_MONOTONIC: i32 = 0x06;
/// timestamp info
pub const SCM_TIME_INFO: i32 = 0x07;
/// process creds (struct sockcred2)
pub const SCM_CREDS2: i32 = 0x08;

/// howto arguments for `shutdown(2)`, specified by Posix.1g.
/// shut down the reading side
pub const SHUT_RD: i32 = 0;
/// shut down the writing side
pub const SHUT_WR: i32 = 1;
/// shut down both sides
pub const SHUT_RDWR: i32 = 2;

/// for SCTP
/// we cheat and use the `SHUT_XX` defines for these
pub const PRU_FLUSH_RD: i32 = SHUT_RD;
pub const PRU_FLUSH_WR: i32 = SHUT_WR;
pub const PRU_FLUSH_RDWR: i32 = SHUT_RDWR;

/// sendfile(2) header/trailer struct
#[repr(C)]
#[derive(Debug, Default)]
pub struct sf_hdtr_t {
    /// pointer to an array of header struct iovec's
    ///
    /// *mut iovec_t
    pub headers: usize,

    /// number of header iovec's
    pub hdr_cnt: i32,

    /// pointer to an array of trailer struct iovec's
    ///
    /// *mut iovec_t
    pub trailers: usize,

    /// number of trailer iovec's
    pub trl_cnt: i32,
}

/// Sendfile-specific flag(s)
pub const SF_NODISKIO: i32 = 0x0000_0001;
/// obsolete
pub const SF_MNOWAIT: i32 = 0x0000_0002;
pub const SF_SYNC: i32 = 0x0000_0004;
pub const SF_USER_READAHEAD: i32 = 0x0000_0008;
pub const SF_NOCACHE: i32 = 0x0000_0010;

#[must_use]
pub const fn SF_READAHEAD(flags: i32) -> i32 {
    flags >> 16
}

/// Sendmmsg/recvmmsg specific structure(s)
#[repr(C)]
#[derive(Debug, Default)]
pub struct mmsghdr_t {
    /// message header
    pub msg_hdr: msghdr_t,

    /// message length
    pub msg_len: ssize_t,
}
