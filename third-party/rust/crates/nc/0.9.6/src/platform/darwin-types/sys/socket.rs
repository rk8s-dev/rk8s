// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/socket.h`
//!
//! Definitions related to sockets: types, address families, options.

use core::ffi::c_void;
use core::mem::size_of;

use crate::{iovec_t, sa_family_t, socklen_t};

/// Types
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

/// Option flags per-socket.
///
/// turn on debugging info recording
pub const SO_DEBUG: i32 = 0x0001;
/// socket has had listen()
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
/// linger on close if data present (in ticks)
pub const SO_LINGER: i32 = 0x0080;
/// leave received OOB data in line
pub const SO_OOBINLINE: i32 = 0x0100;
/// allow local address & port reuse
pub const SO_REUSEPORT: i32 = 0x0200;
/// timestamp received dgram traffic
pub const SO_TIMESTAMP: i32 = 0x0400;
/// Monotonically increasing timestamp on rcvd dgram
pub const SO_TIMESTAMP_MONOTONIC: i32 = 0x0800;
/// there is an accept filter
pub const SO_ACCEPTFILTER: i32 = 0x1000;

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
/// deprecated
pub const SO_LABEL: i32 = 0x1010;
/// deprecated
pub const SO_PEERLABEL: i32 = 0x1011;
/// APPLE: get 1st-packet byte count
pub const SO_NREAD: i32 = 0x1020;
/// APPLE: Install socket-level NKE
pub const SO_NKE: i32 = 0x1021;
/// APPLE: No SIGPIPE on EPIPE
pub const SO_NOSIGPIPE: i32 = 0x1022;
/// APPLE: Returns EADDRNOTAVAIL when src is not available anymore
pub const SO_NOADDRERR: i32 = 0x1023;
/// APPLE: Get number of bytes currently in send socket buffer
pub const SO_NWRITE: i32 = 0x1024;
/// APPLE: Allow reuse of port/socket by different userids
pub const SO_REUSESHAREUID: i32 = 0x1025;
/// linger on close if data present (in seconds)
pub const SO_LINGER_SEC: i32 = 0x1080;
/// APPLE: request local port randomization
pub const SO_RANDOMPORT: i32 = 0x1082;
/// To turn off some POSIX behavior
pub const SO_NP_EXTENSIONS: i32 = 0x1083;

/// number of datagrams in receive socket buffer
pub const SO_NUMRCVPKT: i32 = 0x1112;
/// Network service type
pub const SO_NET_SERVICE_TYPE: i32 = 0x1116;

/// Get `QoS` marking in effect for socket
pub const SO_NETSVC_MARKING_LEVEL: i32 = 0x1119;

/// Best effort
///
/// "Best Effort", unclassified/standard.  This is the default service
/// class and cover the majority of the traffic.
pub const NET_SERVICE_TYPE_BE: i32 = 0;

/// Background system initiated
///
/// "Background", high delay tolerant, loss tolerant. elastic flow,
/// variable size & long-lived. E.g: non-interactive network bulk transfer
/// like synching or backup.
pub const NET_SERVICE_TYPE_BK: i32 = 1;

/// Signaling
///
/// "Signaling", low delay tolerant, low loss tolerant, inelastic flow,
/// jitter tolerant, rate is bursty but short, variable size. E.g. SIP.
pub const NET_SERVICE_TYPE_SIG: i32 = 2;

/// Interactive Video
///
/// "Interactive Video", low delay tolerant, low-medium loss tolerant,
/// elastic flow, constant packet interval, variable rate & size. E.g.
/// video telephony.
pub const NET_SERVICE_TYPE_VI: i32 = 3;

/// Interactive Voice
///
/// "Interactive Voice", very low delay tolerant, very low loss tolerant,
/// inelastic flow, constant packet rate, somewhat fixed size.
/// E.g. `VoIP`.
pub const NET_SERVICE_TYPE_VO: i32 = 4;

/// Responsive Multimedia Audio/Video
///
/// "Responsive Multimedia Audio/Video", low delay tolerant, low-medium
/// loss tolerant, elastic flow, variable packet interval, rate and size.
/// E.g. screen sharing.
pub const NET_SERVICE_TYPE_RV: i32 = 5;

/// Multimedia Audio/Video Streaming
///
/// "Multimedia Audio/Video Streaming", medium delay tolerant, low-medium
/// loss tolerant, elastic flow, constant packet interval, variable rate
/// and size. E.g. video and audio playback with buffering.
pub const NET_SERVICE_TYPE_AV: i32 = 6;

/// Operations, Administration, and Management
///
/// "Operations, Administration, and Management", medium delay tolerant,
/// low-medium loss tolerant, elastic & inelastic flows, variable size.
/// E.g. VPN tunnels.
pub const NET_SERVICE_TYPE_OAM: i32 = 7;

/// Responsive Data
///
/// "Responsive Data", a notch higher than "Best Effort", medium delay
/// tolerant, elastic & inelastic flow, bursty, long-lived. E.g. email,
/// instant messaging, for which there is a sense of interactivity and
/// urgency (user waiting for output).
pub const NET_SERVICE_TYPE_RD: i32 = 8;

/// These are supported values for `SO_NETSVC_MARKING_LEVEL`
/// The outgoing network interface is not known
pub const NETSVC_MRKNG_UNKNOWN: i32 = 0;
/// Default marking at layer 2 (for example Wi-Fi WMM)
pub const NETSVC_MRKNG_LVL_L2: i32 = 1;
/// Layer 3 DSCP marking and layer 2 marking for all Network Service Types
pub const NETSVC_MRKNG_LVL_L3L2_ALL: i32 = 2;
/// The system policy limits layer 3 DSCP marking and layer 2 marking
/// to background Network Service Types */
pub const NETSVC_MRKNG_LVL_L3L2_BK: i32 = 3;

pub type sae_associd_t = u32;
pub const SAE_ASSOCID_ANY: sae_associd_t = 0;
pub const SAE_ASSOCID_ALL: sae_associd_t = u32::MAX;

pub type sae_connid_t = u32;
pub const SAE_CONNID_ANY: sae_connid_t = 0;
pub const SAE_CONNID_ALL: sae_connid_t = u32::MAX;

/// connectx() flag parameters
/// resume connect() on read/write
pub const CONNECT_RESUME_ON_READ_WRITE: i32 = 0x1;
/// data is idempotent
pub const CONNECT_DATA_IDEMPOTENT: i32 = 0x2;
/// data includes security that replaces the TFO-cookie
pub const CONNECT_DATA_AUTHENTICATED: i32 = 0x4;

/// sockaddr endpoints
#[repr(C)]
pub struct sa_endpoints_t {
    /// optional source interface
    pub sae_srcif: u32,
    /// optional source address
    pub sae_srcaddr: *const sockaddr_t,
    /// size of source address
    pub sae_srcaddrlen: socklen_t,
    /// destination address
    pub sae_dstaddr: *const sockaddr_t,
    /// size of destination address
    pub sae_dstaddrlen: socklen_t,
}

/// Structure used for manipulating linger option.
#[repr(C)]
pub struct linger_t {
    /// option on/off
    l_onoff: i32,
    /// linger time
    pub l_linger: i32,
}

/// flag for allowing setsockopt after shutdown
pub const SONPX_SETOPTSHUT: i32 = 0x0000_0001;

/// Level number for (get/set)sockopt() to apply to socket itself.
///
/// options for socket level
pub const SOL_SOCKET: i32 = 0xffff;

/// Address families.
///
/// unspecified
pub const AF_UNSPEC: i32 = 0;
/// local to host (pipes)
pub const AF_UNIX: i32 = 1;
/// backward compatibility
pub const AF_LOCAL: i32 = AF_UNIX;
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
/// Network Driver 'raw' access
pub const AF_NDRV: i32 = 27;
/// Integrated Services Digital Network
pub const AF_ISDN: i32 = 28;
/// CCITT E.164 recommendation
pub const AF_E164: i32 = AF_ISDN;
/// Internal key-management function
pub const PSEUDO_AF_KEY: i32 = 29;
/// IPv6
pub const AF_INET6: i32 = 30;
/// native ATM access
pub const AF_NATM: i32 = 31;
/// Kernel event messages
pub const AF_SYSTEM: i32 = 32;
/// `NetBIOS`
pub const AF_NETBIOS: i32 = 33;
/// PPP communication protocol
pub const AF_PPP: i32 = 34;
/// Used by BPF to not rewrite headers in interface output routine
pub const PSEUDO_AF_HDRCMPLT: i32 = 35;
/// Reserved for internal usage
pub const AF_RESERVED_36: i32 = 36;
/// IEEE 802.11 protocol
pub const AF_IEEE80211: i32 = 37;
pub const AF_UTUN: i32 = 38;
/// VM Sockets
pub const AF_VSOCK: i32 = 40;
pub const AF_MAX: i32 = 41;

/// Structure used by kernel to store most addresses.
#[repr(C)]
pub struct sockaddr_t {
    /// total length
    pub sa_len: u8,
    /// address family
    pub sa_family: sa_family_t,
    /// addr value (actually larger)
    pub sa_data: [u8; 14],
}

/// longest possible addresses
pub const SOCK_MAXADDRLEN: i32 = 255;

/// RFC 2553: protocol-independent placeholder for socket addresses
pub const _SS_MAXSIZE: usize = 128;
pub const _SS_ALIGNSIZE: usize = size_of::<i64>();
pub const _SS_PAD1SIZE: usize = _SS_ALIGNSIZE - size_of::<u8>() - size_of::<sa_family_t>();
pub const _SS_PAD2SIZE: usize =
    _SS_MAXSIZE - size_of::<u8>() - size_of::<sa_family_t>() - _SS_PAD1SIZE - _SS_ALIGNSIZE;

/// `sockaddr_storage`
pub struct sockaddr_storage_t {
    /// address length
    pub ss_len: u8,
    /// address family
    pub ss_family: sa_family_t,
    __ss_pad1: [u8; _SS_PAD1SIZE],
    /// force structure storage alignment
    pub __ss_align: i64,
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
pub const PF_ROUTE: i32 = AF_ROUTE;
pub const PF_LINK: i32 = AF_LINK;
/// really just proto family, no AF
pub const PF_XTP: i32 = PSEUDO_AF_XTP;
pub const PF_COIP: i32 = AF_COIP;
pub const PF_CNT: i32 = AF_CNT;
pub const PF_SIP: i32 = AF_SIP;
/// same format as `AF_NS`
pub const PF_IPX: i32 = AF_IPX;
/// same format as `AF_INET`
pub const PF_RTIP: i32 = PSEUDO_AF_RTIP;
pub const PF_PIP: i32 = PSEUDO_AF_PIP;
pub const PF_NDRV: i32 = AF_NDRV;
pub const PF_ISDN: i32 = AF_ISDN;
pub const PF_KEY: i32 = PSEUDO_AF_KEY;
pub const PF_INET6: i32 = AF_INET6;
pub const PF_NATM: i32 = AF_NATM;
pub const PF_SYSTEM: i32 = AF_SYSTEM;
pub const PF_NETBIOS: i32 = AF_NETBIOS;
pub const PF_PPP: i32 = AF_PPP;
pub const PF_RESERVED_36: i32 = AF_RESERVED_36;
pub const PF_UTUN: i32 = AF_UTUN;
pub const PF_VSOCK: i32 = AF_VSOCK;
pub const PF_MAX: i32 = AF_MAX;

/// These do not have socket-layer support:
///
/// 'vlan'
pub const PF_VLAN: i32 = 0x766c_616e;
/// 'bond'
pub const PF_BOND: i32 = 0x626f_6e64;

pub const NET_MAXID: i32 = AF_MAX;

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
/// survey interface list
pub const NET_RT_IFLIST: i32 = 3;
/// routing statistics
pub const NET_RT_STAT: i32 = 4;
/// routes not in table but not freed
pub const NET_RT_TRASH: i32 = 5;
/// interface list with addresses
pub const NET_RT_IFLIST2: i32 = 6;
/// dump; may limit to a.f.
pub const NET_RT_DUMP2: i32 = 7;

/// Allows read access non-local host's MAC address
/// if the process has neighbor cache entitlement.
pub const NET_RT_FLAGS_PRIV: i32 = 10;
pub const NET_RT_MAXID: i32 = 11;

/// Maximum queue length specifiable by listen.
pub const SOMAXCONN: i32 = 128;

/// Message header for recvmsg and sendmsg calls.
///
/// Used value-result for recvmsg, value only for sendmsg.
#[repr(C)]
pub struct msghdr_t {
    /// [XSI] optional address
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
    /// [XSI] flags on received message
    pub msg_flags: i32,
}

/// process out-of-band data
pub const MSG_OOB: i32 = 0x1;
/// peek at incoming message
pub const MSG_PEEK: i32 = 0x2;
/// send without using routing tables
pub const MSG_DONTROUTE: i32 = 0x4;
/// data completes record
pub const MSG_EOR: i32 = 0x8;
/// data discarded before delivery
pub const MSG_TRUNC: i32 = 0x10;
/// control data lost before delivery
pub const MSG_CTRUNC: i32 = 0x20;
/// wait for full request or error
pub const MSG_WAITALL: i32 = 0x40;
/// this message should be nonblocking
pub const MSG_DONTWAIT: i32 = 0x80;
/// data completes connection
pub const MSG_EOF: i32 = 0x100;
/// Start of 'hold' seq; dump `so_temp`, deprecated
pub const MSG_FLUSH: i32 = 0x400;
/// Hold frag in `so_temp`, deprecated
pub const MSG_HOLD: i32 = 0x800;
/// Send the packet in `so_temp`, deprecated
pub const MSG_SEND: i32 = 0x1000;
/// Data ready to be read
pub const MSG_HAVEMORE: i32 = 0x2000;
/// Data remains in current pkt
pub const MSG_RCVMORE: i32 = 0x4000;
/// Fail receive if socket address cannot be allocated
pub const MSG_NEEDSA: i32 = 0x10000;

/// do not generate SIGPIPE on EOF
pub const MSG_NOSIGNAL: i32 = 0x80000;

/// "Socket"-level control message types:
///
/// access rights (`array of int`)
pub const SCM_RIGHTS: i32 = 0x01;
/// timestamp (`struct timeval`)
pub const SCM_TIMESTAMP: i32 = 0x02;
/// process creds (`struct cmsgcred`)
pub const SCM_CREDS: i32 = 0x03;
/// timestamp (`uint64_t`)
pub const SCM_TIMESTAMP_MONOTONIC: i32 = 0x04;

/// howto arguments for shutdown(2), specified by Posix.1g.
///
/// shut down the reading side
pub const SHUT_RD: i32 = 0;
/// shut down the writing side
pub const SHUT_WR: i32 = 1;
/// shut down both sides
pub const SHUT_RDWR: i32 = 2;

/// sendfile(2) header/trailer struct
#[repr(C)]
pub struct sf_hdtr_t {
    /// pointer to an array of header struct iovec's
    pub headers: *mut iovec_t,
    /// number of header iovec's
    pub hdr_cnt: i32,
    /// pointer to an array of trailer struct iovec's
    pub trailers: *mut iovec_t,
    /// number of trailer iovec's
    pub trl_cnt: i32,
}
