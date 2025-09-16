// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/socket.h`
//!
//! Definitions related to sockets: types, address families, options.

#![allow(overflowing_literals)]

use core::ffi::c_void;
use core::mem::size_of;

use crate::{gid_t, iovec_t, pid_t, sa_family_t, socklen_t, uid_t};

/// Socket types.
///
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
/// connection-orientated datagram
pub const SOCK_CONN_DGRAM: i32 = 6;
pub const SOCK_DCCP: i32 = SOCK_CONN_DGRAM;

/// set close on exec on socket
pub const SOCK_CLOEXEC: i32 = 0x1000_0000;
/// set non blocking i/o socket
pub const SOCK_NONBLOCK: i32 = 0x2000_0000;
/// don't send sigpipe
pub const SOCK_NOSIGPIPE: i32 = 0x4000_0000;
/// flags mask
pub const SOCK_FLAGS_MASK: i32 = 0xf000_0000;

/// Option flags per-socket.
///
/// turn on debugging info recording
pub const SO_DEBUG: i32 = 0x0001;
/// socket has had `listen()`
pub const SO_ACCEPTCONN: i32 = 0x0002;
/// allow local address reuse
pub const SO_REUSEADDR: i32 = 0x0004;
/// keep connections alive
pub const SO_KEEPALIVE: i32 = 0x0008;
/// just use interface addresses
pub const SO_DONTROUTE: i32 = 0x0010;
/// permit sending of broadcast msgs
pub const SO_BROADCAST: i32 = 0x0020;
/// bypass hardware when possible
pub const SO_USELOOPBACK: i32 = 0x0040;
/// linger on close if data present
pub const SO_LINGER: i32 = 0x0080;
/// leave received OOB data in line
pub const SO_OOBINLINE: i32 = 0x0100;
/// allow local address & port reuse
pub const SO_REUSEPORT: i32 = 0x0200;
// SO_OTIMESTAMP	0x0400
/// no SIGPIPE from EPIPE
pub const SO_NOSIGPIPE: i32 = 0x0800;
/// there is an accept filter
pub const SO_ACCEPTFILTER: i32 = 0x1000;
/// timestamp received dgram traffic
pub const SO_TIMESTAMP: i32 = 0x2000;
/// Keep track of receive errors
pub const SO_RERROR: i32 = 0x4000;

/// Allowed default option flags
pub const SO_DEFOPTS: i32 = SO_DEBUG
    | SO_REUSEADDR
    | SO_KEEPALIVE
    | SO_DONTROUTE
    | SO_BROADCAST
    | SO_USELOOPBACK
    | SO_LINGER
    | SO_OOBINLINE
    | SO_REUSEPORT
    | SO_NOSIGPIPE
    | SO_TIMESTAMP
    | SO_RERROR;

/// Additional options, not kept in `so_options`.
///
/// send buffer size
pub const SO_SNDBUF: i32 = 0x1001;
/// receive buffer size
pub const SO_RCVBUF: i32 = 0x1002;
/// send low-water mark
pub const SO_SNDLOWAT: i32 = 0x1003;
/// receive low-water mark
pub const SO_RCVLOWAT: i32 = 0x1004;
// SO_OSNDTIMEO		0x1005
// SO_ORCVTIMEO		0x1006
/// get error status and clear
pub const SO_ERROR: i32 = 0x1007;
/// get socket type
pub const SO_TYPE: i32 = 0x1008;
/// datagrams: return packets dropped
pub const SO_OVERFLOWED: i32 = 0x1009;

/// user supplies no header to kernel; kernel removes header and supplies payload
pub const SO_NOHEADER: i32 = 0x100a;
/// send timeout
pub const SO_SNDTIMEO: i32 = 0x100b;
/// receive timeout
pub const SO_RCVTIMEO: i32 = 0x100c;
/// Structure used for manipulating linger option.
#[repr(C)]
pub struct linger_t {
    /// option on/off
    pub l_onoff: i32,
    /// linger time in seconds
    pub l_linger: i32,
}

#[repr(C)]
pub struct accept_filter_arg_t {
    pub af_name: [u8; 16],
    pub af_arg: [u8; 256 - 16],
}

/// Level number for (get/set)sockopt() to apply to socket itself.
///
/// options for socket level
pub const SOL_SOCKET: i32 = 0xffff;

/// RFC 2553: protocol-independent placeholder for socket addresses
pub const _SS_MAXSIZE: usize = 128;
pub const _SS_ALIGNSIZE: usize = size_of::<i64>();
pub const _SS_PAD1SIZE: usize = _SS_ALIGNSIZE - 2;
pub const _SS_PAD2SIZE: usize = _SS_MAXSIZE - 2 - _SS_PAD1SIZE - _SS_ALIGNSIZE;

/// Address families.
///
/// unspecified
pub const AF_UNSPEC: i32 = 0;
/// local to host
pub const AF_LOCAL: i32 = 1;
/// backward compatibility
pub const AF_UNIX: i32 = AF_LOCAL;
/// internetwork: UDP, TCP, etc.
pub const AF_INET: i32 = 2;
/// arpanet imp addresses
pub const AF_IMPLINK: i32 = 3;
/// pup protocols: e.g. BSP
pub const AF_PUP: i32 = 4;
/// mit CHAOS protocols
pub const AF_CHAOS: i32 = 5;
/// XEROX NS protocols
pub const AF_NS: i32 = 6;
/// ISO protocols
pub const AF_ISO: i32 = 7;
pub const AF_OSI: i32 = AF_ISO;
/// european computer manufacturers
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
pub const AF_OROUTE: i32 = 17;
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
/// IP version 6
pub const AF_INET6: i32 = 24;
/// Help Identify PIP packets
pub const PSEUDO_AF_PIP: i32 = 25;
/// Integrated Services Digital Network
pub const AF_ISDN: i32 = 26;
/// CCITT E.164 recommendation
pub const AF_E164: i32 = AF_ISDN;
/// native ATM access
pub const AF_NATM: i32 = 27;
/// (rev.) addr. res. prot. (RFC 826)
pub const AF_ARP: i32 = 28;
/// Internal key management protocol
pub const PSEUDO_AF_KEY: i32 = 29;
/// Used by BPF to not rewrite hdrs in interface output routine
pub const PSEUDO_AF_HDRCMPLT: i32 = 30;
/// Bluetooth: HCI, SCO, L2CAP, RFCOMM
pub const AF_BLUETOOTH: i32 = 31;
/// IEEE80211
pub const AF_IEEE80211: i32 = 32;
/// `MultiProtocol` Label Switching
pub const AF_MPLS: i32 = 33;
/// Internal Routing Protocol
pub const AF_ROUTE: i32 = 34;
pub const AF_CAN: i32 = 35;
pub const AF_ETHER: i32 = 36;
pub const AF_MAX: i32 = 37;

/// Structure used by kernel to store most addresses.
#[repr(C)]
pub struct sockaddr_t {
    /// total length
    pub sa_len: u8,
    /// address family
    pub sa_family: sa_family_t,
    /// actually longer; address value
    pub sa_data: [u8; 14],
}

/// Structure used by kernel to pass protocol information in raw sockets.
#[repr(C)]
pub struct sockproto_t {
    /// address family
    pub sp_family: u16,
    /// protocol
    pub sp_protocol: u16,
}

/// we make the entire struct at least `UCHAR_MAX + 1` in size since existing
/// use of `sockaddr_un` permits a path up to 253 bytes + '\0'.
/// `sizeof(sb_len) + sizeof(sb_family) + 253 + '\0'`
pub const _SB_DATASIZE: i32 = 254;

#[repr(C)]
pub struct sockaddr_storage_t {
    /// address length
    pub ss_len: u8,
    /// address family
    pub ss_family: sa_family_t,
    __ss_pad1: [u8; _SS_PAD1SIZE],
    /// force desired structure storage alignment
    __ss_align: i64,
    __ss_pad2: [u8; _SS_PAD2SIZE],
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
pub const PF_NS: i32 = AF_NS;
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
pub const PF_OROUTE: i32 = AF_OROUTE;
pub const PF_LINK: i32 = AF_LINK;
/// really just proto family, no AF
pub const PF_XTP: i32 = PSEUDO_AF_XTP;
pub const PF_COIP: i32 = AF_COIP;
pub const PF_CNT: i32 = AF_CNT;
pub const PF_INET6: i32 = AF_INET6;
/// same format as `AF_NS`
pub const PF_IPX: i32 = AF_IPX;
/// same format as `AF_INET`
pub const PF_RTIP: i32 = PSEUDO_AF_RTIP;
pub const PF_PIP: i32 = PSEUDO_AF_PIP;
/// same as E164
pub const PF_ISDN: i32 = AF_ISDN;
pub const PF_E164: i32 = AF_E164;
pub const PF_NATM: i32 = AF_NATM;
pub const PF_ARP: i32 = AF_ARP;
/// like `PF_ROUTE`, only for key mgmt
pub const PF_KEY: i32 = PSEUDO_AF_KEY;
pub const PF_BLUETOOTH: i32 = AF_BLUETOOTH;
pub const PF_MPLS: i32 = AF_MPLS;
pub const PF_ROUTE: i32 = AF_ROUTE;
pub const PF_CAN: i32 = AF_CAN;
pub const PF_ETHER: i32 = AF_ETHER;

pub const PF_MAX: i32 = AF_MAX;

/// Socket credentials.
pub struct sockcred_t {
    /// process id
    pub sc_pid: pid_t,
    /// real user id
    pub sc_uid: uid_t,
    /// effective user id
    pub sc_euid: uid_t,
    /// real group id
    pub sc_gid: gid_t,
    /// effective group id
    pub sc_egid: gid_t,
    /// number of supplemental groups
    pub sc_ngroups: i32,
    /// variable length
    pub sc_groups: *mut gid_t,
}

pub const PCB_SLOP: i32 = 20;
pub const PCB_ALL: i32 = 0;

/// `PF_ROUTE` - Routing table
///
/// Three additional levels are defined:
/// Fourth: address family, 0 is wildcard
/// Fifth: type of info, defined below
/// Sixth: flag(s) to mask with for `NET_RT_FLAGS`
///
/// dump; may limit to a.f.
pub const NET_RT_DUMP: i32 = 1;
/// by flags, e.g. RESOLVING
pub const NET_RT_FLAGS: i32 = 2;
/// old `NET_RT_IFLIST` (pre 1.5)
pub const NET_RT_OOOIFLIST: i32 = 3;
/// old `NET_RT_IFLIST` (pre-64bit time)
pub const NET_RT_OOIFLIST: i32 = 4;
/// old `NET_RT_IFLIST` (pre 8.0)
pub const NET_RT_OIFLIST: i32 = 5;
/// survey interface list
pub const NET_RT_IFLIST: i32 = 6;

/// Maximum queue length specifiable by listen(2).
pub const SOMAXCONN: i32 = 128;

/// Message header for recvmsg and sendmsg calls.
/// Used value-result for recvmsg, value only for sendmsg.
pub struct msghdr_t {
    /// optional address
    pub msg_name: *mut c_void,
    /// size of address
    pub msg_namelen: socklen_t,
    /// scatter/gather array
    pub msg_iov: *mut iovec_t,
    /// # elements in msg_iov
    pub msg_iovlen: i32,
    /// ancillary data, see below
    pub msg_control: *mut c_void,
    /// ancillary data buffer len
    pub msg_controllen: socklen_t,
    /// flags on received message
    pub msg_flags: i32,
}

/// process out-of-band data
pub const MSG_OOB: i32 = 0x0001;
/// peek at incoming message
pub const MSG_PEEK: i32 = 0x0002;
/// send without using routing tables
pub const MSG_DONTROUTE: i32 = 0x0004;
/// data completes record
pub const MSG_EOR: i32 = 0x0008;
/// data discarded before delivery
pub const MSG_TRUNC: i32 = 0x0010;
/// control data lost before delivery
pub const MSG_CTRUNC: i32 = 0x0020;
/// wait for full request or error
pub const MSG_WAITALL: i32 = 0x0040;
/// this message should be nonblocking
pub const MSG_DONTWAIT: i32 = 0x0080;
/// this message was rcvd using link-level brdcst
pub const MSG_BCAST: i32 = 0x0100;
/// this message was rcvd using link-level mcast
pub const MSG_MCAST: i32 = 0x0200;
/// do not generate SIGPIPE on EOF
pub const MSG_NOSIGNAL: i32 = 0x0400;
/// close on exec receiving fd
pub const MSG_CMSG_CLOEXEC: i32 = 0x0800;
/// use non-blocking I/O
pub const MSG_NBIO: i32 = 0x1000;
/// recvmmsg() wait for one message
pub const MSG_WAITFORONE: i32 = 0x2000;
/// SCTP notification
pub const MSG_NOTIFICATION: i32 = 0x4000;

#[repr(C)]
pub struct mmsghdr_t {
    pub msg_hdr: msghdr_t,
    pub msg_len: u32,
}

/// Extra flags used internally only
pub const MSG_USERFLAGS: i32 = 0x00ff_ffff;
/// `msg_name` is an mbuf
pub const MSG_NAMEMBUF: i32 = 0x0100_0000;
/// `msg_control` is an mbuf
pub const MSG_CONTROLMBUF: i32 = 0x0200_0000;
/// `msg_iov` is in user space
pub const MSG_IOVUSRSPACE: i32 = 0x0400_0000;
/// address length is in user space
pub const MSG_LENUSRSPACE: i32 = 0x0800_0000;

/// Header for ancillary data objects in `msg_control` buffer.
/// Used for additional information with/about a datagram
/// not expressible by flags.  The format is a sequence
/// of message elements headed by cmsghdr structures.
pub struct cmsghdr_t {
    /// data byte count, including hdr
    pub cmsg_len: socklen_t,
    /// originating protocol
    pub cmsg_level: i32,
    /// protocol-specific type
    pub cmsg_type: i32,
    // followed by	u_char  cmsg_data[];
}

/// Alignment requirement for CMSG struct manipulation.
///
/// This basically behaves the same as ALIGN() ARCH/include/param.h.
/// We declare it separately for two reasons:
/// (1) avoid dependency between machine/param.h, and (2) to sync with kernel's
/// idea of ALIGNBYTES at runtime.
/// without (2), we can't guarantee binary compatibility in case of future
/// changes in ALIGNBYTES.
///
/// "Socket"-level control message types:
/// access rights (array of int)
pub const SCM_RIGHTS: i32 = 0x01;
// 0x02		   timestamp (struct timeval50)
// 0x04		   credentials (struct sockcred70)
/// timestamp (struct timeval)
pub const SCM_TIMESTAMP: i32 = 0x08;
/// credentials (struct sockcred)
pub const SCM_CREDS: i32 = 0x10;

/// Types of socket shutdown(2).
///
/// Disallow further receives.
pub const SHUT_RD: i32 = 0;
/// Disallow further sends.
pub const SHUT_WR: i32 = 1;
/// Disallow further sends/receives.
pub const SHUT_RDWR: i32 = 2;
