// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `netinet/in.h`

#![allow(clippy::module_name_repetitions)]

use crate::{in_addr_t, in_port_t, sa_family_t, sockaddr_storage_t};

/// Protocols (RFC 1700)
///
/// dummy for IP
pub const IPPROTO_IP: i32 = 0;
/// IP6 hop-by-hop options
pub const IPPROTO_HOPOPTS: i32 = 0;
/// control message protocol
pub const IPPROTO_ICMP: i32 = 1;
/// group mgmt protocol
pub const IPPROTO_IGMP: i32 = 2;
/// gateway^2 (deprecated)
pub const IPPROTO_GGP: i32 = 3;
/// IPv4 encapsulation
pub const IPPROTO_IPV4: i32 = 4;
/// for compatibility
pub const IPPROTO_IPIP: i32 = IPPROTO_IPV4;
/// tcp
pub const IPPROTO_TCP: i32 = 6;
/// Stream protocol II
pub const IPPROTO_ST: i32 = 7;
/// exterior gateway protocol
pub const IPPROTO_EGP: i32 = 8;
/// private interior gateway
pub const IPPROTO_PIGP: i32 = 9;
/// BBN RCC Monitoring
pub const IPPROTO_RCCMON: i32 = 10;
/// network voice protocol
pub const IPPROTO_NVPII: i32 = 11;
/// pup
pub const IPPROTO_PUP: i32 = 12;
/// Argus
pub const IPPROTO_ARGUS: i32 = 13;
/// EMCON
pub const IPPROTO_EMCON: i32 = 14;
/// Cross Net Debugger
pub const IPPROTO_XNET: i32 = 15;
/// Chaos
pub const IPPROTO_CHAOS: i32 = 16;
/// user datagram protocol
pub const IPPROTO_UDP: i32 = 17;
/// Multiplexing
pub const IPPROTO_MUX: i32 = 18;
/// DCN Measurement Subsystems
pub const IPPROTO_MEAS: i32 = 19;
/// Host Monitoring
pub const IPPROTO_HMP: i32 = 20;
/// Packet Radio Measurement
pub const IPPROTO_PRM: i32 = 21;
/// xns idp
pub const IPPROTO_IDP: i32 = 22;
/// Trunk-1
pub const IPPROTO_TRUNK1: i32 = 23;
/// Trunk-2
pub const IPPROTO_TRUNK2: i32 = 24;
/// Leaf-1
pub const IPPROTO_LEAF1: i32 = 25;
/// Leaf-2
pub const IPPROTO_LEAF2: i32 = 26;
/// Reliable Data
pub const IPPROTO_RDP: i32 = 27;
/// Reliable Transaction
pub const IPPROTO_IRTP: i32 = 28;
/// tp-4 w/ class negotiation
pub const IPPROTO_TP: i32 = 29;
/// Bulk Data Transfer
pub const IPPROTO_BLT: i32 = 30;
/// Network Services
pub const IPPROTO_NSP: i32 = 31;
/// Merit Internodal
pub const IPPROTO_INP: i32 = 32;
/// Sequential Exchange
pub const IPPROTO_SEP: i32 = 33;
/// Third Party Connect
pub const IPPROTO_3PC: i32 = 34;
/// `InterDomain` Policy Routing
pub const IPPROTO_IDPR: i32 = 35;
/// XTP
pub const IPPROTO_XTP: i32 = 36;
/// Datagram Delivery
pub const IPPROTO_DDP: i32 = 37;
/// Control Message Transport
pub const IPPROTO_CMTP: i32 = 38;
/// TP++ Transport
pub const IPPROTO_TPXX: i32 = 39;
/// IL transport protocol
pub const IPPROTO_IL: i32 = 40;
/// IP6 header
pub const IPPROTO_IPV6: i32 = 41;
/// Source Demand Routing
pub const IPPROTO_SDRP: i32 = 42;
/// IP6 routing header
pub const IPPROTO_ROUTING: i32 = 43;
/// IP6 fragmentation header
pub const IPPROTO_FRAGMENT: i32 = 44;
/// `InterDomain` Routing
pub const IPPROTO_IDRP: i32 = 45;
/// resource reservation
pub const IPPROTO_RSVP: i32 = 46;
/// General Routing Encap.
pub const IPPROTO_GRE: i32 = 47;
/// Mobile Host Routing
pub const IPPROTO_MHRP: i32 = 48;
/// BHA
pub const IPPROTO_BHA: i32 = 49;
/// IP6 Encap Sec. Payload
pub const IPPROTO_ESP: i32 = 50;
/// IP6 Auth Header
pub const IPPROTO_AH: i32 = 51;
/// Integ. Net Layer Security
pub const IPPROTO_INLSP: i32 = 52;
/// IP with encryption
pub const IPPROTO_SWIPE: i32 = 53;
/// Next Hop Resolution
pub const IPPROTO_NHRP: i32 = 54;
/// 55-57: Unassigned
/// ICMP6
pub const IPPROTO_ICMPV6: i32 = 58;
/// IP6 no next header
pub const IPPROTO_NONE: i32 = 59;
/// IP6 destination option
pub const IPPROTO_DSTOPTS: i32 = 60;
/// any host internal protocol
pub const IPPROTO_AHIP: i32 = 61;
/// CFTP
pub const IPPROTO_CFTP: i32 = 62;
/// "hello" routing protocol
pub const IPPROTO_HELLO: i32 = 63;
/// SATNET/Backroom EXPAK
pub const IPPROTO_SATEXPAK: i32 = 64;
/// Kryptolan
pub const IPPROTO_KRYPTOLAN: i32 = 65;
/// Remote Virtual Disk
pub const IPPROTO_RVD: i32 = 66;
/// Pluribus Packet Core
pub const IPPROTO_IPPC: i32 = 67;
/// Any distributed FS
pub const IPPROTO_ADFS: i32 = 68;
/// Satnet Monitoring
pub const IPPROTO_SATMON: i32 = 69;
/// VISA Protocol
pub const IPPROTO_VISA: i32 = 70;
/// Packet Core Utility
pub const IPPROTO_IPCV: i32 = 71;
/// Comp. Prot. Net. Executive
pub const IPPROTO_CPNX: i32 = 72;
/// Comp. Prot. `HeartBeat`
pub const IPPROTO_CPHB: i32 = 73;
/// Wang Span Network
pub const IPPROTO_WSN: i32 = 74;
/// Packet Video Protocol
pub const IPPROTO_PVP: i32 = 75;
/// `BackRoom` SATNET Monitoring
pub const IPPROTO_BRSATMON: i32 = 76;
/// Sun net disk proto (temp.)
pub const IPPROTO_ND: i32 = 77;
/// WIDEBAND Monitoring
pub const IPPROTO_WBMON: i32 = 78;
/// WIDEBAND EXPAK
pub const IPPROTO_WBEXPAK: i32 = 79;
/// ISO cnlp
pub const IPPROTO_EON: i32 = 80;
/// VMTP
pub const IPPROTO_VMTP: i32 = 81;
/// Secure VMTP
pub const IPPROTO_SVMTP: i32 = 82;
/// Banyon VINES
pub const IPPROTO_VINES: i32 = 83;
/// TTP
pub const IPPROTO_TTP: i32 = 84;
/// NSFNET-IGP
pub const IPPROTO_IGP: i32 = 85;
/// dissimilar gateway prot.
pub const IPPROTO_DGP: i32 = 86;
/// TCF
pub const IPPROTO_TCF: i32 = 87;
/// Cisco/GXS IGRP
pub const IPPROTO_IGRP: i32 = 88;
/// OSPFIGP
pub const IPPROTO_OSPFIGP: i32 = 89;
/// Strite RPC protocol
pub const IPPROTO_SRPC: i32 = 90;
/// Locus Address Resoloution
pub const IPPROTO_LARP: i32 = 91;
/// Multicast Transport
pub const IPPROTO_MTP: i32 = 92;
/// AX.25 Frames
pub const IPPROTO_AX25: i32 = 93;
/// IP encapsulated in IP
pub const IPPROTO_IPEIP: i32 = 94;
/// Mobile Int.ing control
pub const IPPROTO_MICP: i32 = 95;
/// Semaphore Comm. security
pub const IPPROTO_SCCSP: i32 = 96;
/// Ethernet IP encapsulation
pub const IPPROTO_ETHERIP: i32 = 97;
/// encapsulation header
pub const IPPROTO_ENCAP: i32 = 98;
/// any private encr. scheme
pub const IPPROTO_APES: i32 = 99;
/// GMTP
pub const IPPROTO_GMTP: i32 = 100;
/// 101-252: Partly Unassigned
/// Protocol Independent Mcast
pub const IPPROTO_PIM: i32 = 103;
/// payload compression (`IPComp`)
pub const IPPROTO_IPCOMP: i32 = 108;
/// PGM
pub const IPPROTO_PGM: i32 = 113;
/// SCTP
pub const IPPROTO_SCTP: i32 = 132;
/// 253-254: Experimentation and testing; 255: Reserved (RFC3692)
/// BSD Private, local use, namespace incursion
/// divert pseudo-protocol
pub const IPPROTO_DIVERT: i32 = 254;
/// raw IP packet
pub const IPPROTO_RAW: i32 = 255;

pub const IPPROTO_MAX: i32 = 256;

/// last return value of *_input(), meaning "all job for this pkt is done".
pub const IPPROTO_DONE: i32 = 257;

/// Local port number conventions:
///
/// When a user does a bind(2) or connect(2) with a port number of zero,
/// a non-conflicting local port address is chosen.
/// The default range is `IPPORT_RESERVED` through
/// `IPPORT_USERRESERVED`, although that is settable by sysctl.
///
/// A user may set the `IPPROTO_IP` option `IP_PORTRANGE` to change this
/// default assignment range.
///
/// The value `IP_PORTRANGE_DEFAULT` causes the default behavior.
///
/// The value `IP_PORTRANGE_HIGH` changes the range of candidate port numbers
/// into the "high" range.  These are reserved for client outbound connections
/// which do not want to be filtered by any firewalls.
///
/// The value `IP_PORTRANGE_LOW` changes the range to the "low" are
/// that is (by convention) restricted to privileged processes.  This
/// convention is based on "vouchsafe" principles only.  It is only secure
/// if you trust the remote host to restrict these ports.
///
/// The default range of ports and the high range can be changed by
/// sysctl(3).  `(net.inet.ip.port{hi,low}{first,last}_auto)`
///
/// Changing those values has bad security implications if you are
/// using a a stateless firewall that is allowing packets outside of that
/// range in order to allow transparent outgoing connections.
///
/// Such a firewall configuration will generally depend on the use of these
/// default values.  If you change them, you may find your Security
/// Administrator looking for you with a heavy object.
///
/// For a slightly more orthodox text view on this:
///
///            ftp://ftp.isi.edu/in-notes/iana/assignments/port-numbers
///
///    port numbers are divided into three ranges:
///
///                0 -  1023 Well Known Ports
///             1024 - 49151 Registered Ports
///            49152 - 65535 Dynamic and/or Private Ports
///
///
pub const __DARWIN_IPPORT_RESERVED: in_addr_t = 1024;

/// Ports < `IPPORT_RESERVED` are reserved for privileged processes (e.g. root).
///
/// (`IP_PORTRANGE_LOW`)
pub const IPPORT_RESERVED: in_addr_t = __DARWIN_IPPORT_RESERVED;
/// Ports > `IPPORT_USERRESERVED` are reserved for servers, not necessarily privileged.
///
/// (`IP_PORTRANGE_DEFAULT`)
pub const IPPORT_USERRESERVED: in_addr_t = 5000;

/// Default local port range to use by setting `IP_PORTRANGE_HIGH`
pub const IPPORT_HIFIRSTAUTO: in_addr_t = 49152;
pub const IPPORT_HILASTAUTO: in_addr_t = 65535;

/// Scanning for a free reserved port return a value below `IPPORT_RESERVED`,
/// but higher than `IPPORT_RESERVEDSTART`.  Traditionally the start value was
/// 512, but that conflicts with some well-known-services that firewalls may
/// have a fit if we use.
pub const IPPORT_RESERVEDSTART: in_addr_t = 600;

/// Internet address (a structure for historical reasons)
#[repr(C)]
pub struct in_addr_s {
    pub s_addr: in_addr_t,
}

/// Definitions of bits in internet address integers.
/// On subnets, the decomposition of addresses to host and net parts
/// is done according to subnet mask, not the masks here.
pub const INADDR_ANY: in_addr_t = 0x0000_0000;
/// must be masked
pub const INADDR_BROADCAST: in_addr_t = 0xffff_ffff;

#[must_use]
pub const fn IN_CLASSA(i: in_addr_t) -> bool {
    (i & 0x8000_0000) == 0
}
pub const IN_CLASSA_NET: in_addr_t = 0xff00_0000;
pub const IN_CLASSA_NSHIFT: usize = 24;
pub const IN_CLASSA_HOST: in_addr_t = 0x00ff_ffff;
pub const IN_CLASSA_MAX: usize = 128;

#[must_use]
pub const fn IN_CLASSB(i: in_addr_t) -> bool {
    (i & 0xc000_0000) == 0x8000_0000
}
pub const IN_CLASSB_NET: in_addr_t = 0xffff_0000;
pub const IN_CLASSB_NSHIFT: usize = 16;
pub const IN_CLASSB_HOST: in_addr_t = 0x0000_ffff;
pub const IN_CLASSB_MAX: usize = 65536;

#[must_use]
pub const fn IN_CLASSC(i: in_addr_t) -> bool {
    (i & 0xe000_0000) == 0xc000_0000
}
pub const IN_CLASSC_NET: in_addr_t = 0xffff_ff00;
pub const IN_CLASSC_NSHIFT: usize = 8;
pub const IN_CLASSC_HOST: in_addr_t = 0x0000_00ff;

#[must_use]
pub const fn IN_CLASSD(i: in_addr_t) -> bool {
    (i & 0xf000_0000) == 0xe000_0000
}
/// These ones aren't really
pub const IN_CLASSD_NET: in_addr_t = 0xf000_0000;
/// net and host fields, but
pub const IN_CLASSD_NSHIFT: usize = 28;
/// routing needn't know.
pub const IN_CLASSD_HOST: in_addr_t = 0x0fff_ffff;

#[must_use]
#[inline]
pub const fn IN_MULTICAST(i: in_addr_t) -> bool {
    IN_CLASSD(i)
}

#[must_use]
pub const fn IN_EXPERIMENTAL(i: in_addr_t) -> bool {
    (i & 0xf000_0000) == 0xf000_0000
}

#[must_use]
pub const fn IN_BADCLASS(i: in_addr_t) -> bool {
    (i & 0xf000_0000) == 0xf000_0000
}

pub const INADDR_LOOPBACK: in_addr_t = 0x7f00_0001;

/// -1 return
pub const INADDR_NONE: in_addr_t = 0xffff_ffff;

/// 224.0.0.0
pub const INADDR_UNSPEC_GROUP: in_addr_t = 0xe000_0000;

/// 224.0.0.1
pub const INADDR_ALLHOSTS_GROUP: in_addr_t = 0xe000_0001;

/// 224.0.0.2
pub const INADDR_ALLRTRS_GROUP: in_addr_t = 0xe000_0002;

/// 224.0.0.22, `IGMPv3`
pub const INADDR_ALLRPTS_GROUP: in_addr_t = 0xe000_0016;

/// 224.0.0.18
pub const INADDR_CARP_GROUP: in_addr_t = 0xe000_0012;

/// 224.0.0.240
pub const INADDR_PFSYNC_GROUP: in_addr_t = 0xe000_00f0;

/// 224.0.0.251
pub const INADDR_ALLMDNS_GROUP: in_addr_t = 0xe000_00fb;

/// 224.0.0.255
pub const INADDR_MAX_LOCAL_GROUP: in_addr_t = 0xe000_00ff;

/// 169.254.0.0
pub const IN_LINKLOCALNETNUM: in_addr_t = 0xA9FE_0000;

#[must_use]
pub const fn IN_LINKLOCAL(i: in_addr_t) -> bool {
    (i & IN_CLASSB_NET) == IN_LINKLOCALNETNUM
}

#[must_use]
pub const fn IN_LOOPBACK(i: in_addr_t) -> bool {
    (i & 0xff00_0000) == 0x7f00_0000
}

#[must_use]
pub const fn IN_ZERONET(i: in_addr_t) -> bool {
    (i & 0xff00_0000) == 0
}

#[must_use]
pub const fn IN_PRIVATE(i: in_addr_t) -> bool {
    ((i & 0xff00_0000) == 0x0a00_0000)
        || ((i & 0xfff0_0000) == 0xac10_0000)
        || ((i & 0xffff_0000) == 0xc0a8_0000)
}

#[must_use]
pub const fn IN_LOCAL_GROUP(i: in_addr_t) -> bool {
    (i & 0xffff_ff00) == 0xe000_0000
}

#[must_use]
pub const fn IN_ANY_LOCAL(i: in_addr_t) -> bool {
    IN_LINKLOCAL(i) || IN_LOCAL_GROUP(i)
}

/// official!
pub const IN_LOOPBACKNET: in_addr_t = 127;

/// Socket address, internet style.
#[repr(C)]
pub struct sockaddr_in_t {
    pub sin_len: u8,
    pub sin_family: sa_family_t,
    pub sin_port: in_port_t,
    pub sin_addr: in_addr_t,
    pub sin_zero: [u8; 8],
}

pub const INET_ADDRSTRLEN: usize = 16;

/// Structure used to describe IP options.
///
/// Used to store options internally, to pass them to a process,
/// or to restore options retrieved earlier.
/// The `ip_dst` is used for the first-hop gateway when using a source route
/// (this gets put into the header proper).
#[repr(C)]
pub struct ip_opts_t {
    /// first hop, 0 w/o src rt
    pub ip_dst: in_addr_t,
    /// actually variable in size
    pub ip_opts: [u8; 40],
}

/// Options for use with `[gs]etsockopt` at the IP level.
/// First word of comment is data type; bool is stored in int.
///
/// `buf/ip_opts`; set/get IP options
pub const IP_OPTIONS: i32 = 1;
/// int; header is included with data
pub const IP_HDRINCL: i32 = 2;
/// int; IP type of service and preced.
pub const IP_TOS: i32 = 3;
/// int; IP time to live
pub const IP_TTL: i32 = 4;
/// bool; receive all IP opts w/dgram
pub const IP_RECVOPTS: i32 = 5;
/// bool; receive IP opts for response
pub const IP_RECVRETOPTS: i32 = 6;
/// bool; receive IP dst addr w/dgram
pub const IP_RECVDSTADDR: i32 = 7;
/// `ip_opts`; set/get IP options
pub const IP_RETOPTS: i32 = 8;
/// `u_char`; set/get IP multicast i/f
pub const IP_MULTICAST_IF: i32 = 9;
/// `u_char`; set/get IP multicast ttl
pub const IP_MULTICAST_TTL: i32 = 10;
/// `u_char`; set/get IP multicast loopback
pub const IP_MULTICAST_LOOP: i32 = 11;
/// `ip_mreq`; add an IP group membership
pub const IP_ADD_MEMBERSHIP: i32 = 12;
/// `ip_mreq`; drop an IP group membership
pub const IP_DROP_MEMBERSHIP: i32 = 13;
/// set/get IP mcast virt. iface
pub const IP_MULTICAST_VIF: i32 = 14;
/// enable RSVP in kernel
pub const IP_RSVP_ON: i32 = 15;
/// disable RSVP in kernel
pub const IP_RSVP_OFF: i32 = 16;
/// set RSVP per-vif socket
pub const IP_RSVP_VIF_ON: i32 = 17;
/// unset RSVP per-vif socket
pub const IP_RSVP_VIF_OFF: i32 = 18;
/// int; range to choose for unspec port
pub const IP_PORTRANGE: i32 = 19;
/// bool; receive reception if w/dgram
pub const IP_RECVIF: i32 = 20;
/// for IPSEC
/// int; set/get security policy
pub const IP_IPSEC_POLICY: i32 = 21;
/// deprecated
pub const IP_FAITH: i32 = 22;
/// bool: drop receive of raw IP header
pub const IP_STRIPHDR: i32 = 23;
/// bool; receive reception TTL w/dgram
pub const IP_RECVTTL: i32 = 24;
/// int; set/get bound interface
pub const IP_BOUND_IF: i32 = 25;
/// get pktinfo on recv socket, set src on sent dgram
pub const IP_PKTINFO: i32 = 26;
/// receive pktinfo w/dgram
pub const IP_RECVPKTINFO: i32 = IP_PKTINFO;
/// bool; receive IP TOS w/dgram
pub const IP_RECVTOS: i32 = 27;
/// don't fragment packet
pub const IP_DONTFRAG: i32 = 28;

/// add a firewall rule to chain
pub const IP_FW_ADD: i32 = 40;
/// delete a firewall rule from chain
pub const IP_FW_DEL: i32 = 41;
/// flush firewall rule chain
pub const IP_FW_FLUSH: i32 = 42;
/// clear single/all firewall counter(s)
pub const IP_FW_ZERO: i32 = 43;
/// get entire firewall rule chain
pub const IP_FW_GET: i32 = 44;
/// reset logging counters
pub const IP_FW_RESETLOG: i32 = 45;

/// These older firewall socket option codes are maintained for backward compatibility.
/// add a firewall rule to chain
pub const IP_OLD_FW_ADD: i32 = 50;
/// delete a firewall rule from chain
pub const IP_OLD_FW_DEL: i32 = 51;
/// flush firewall rule chain
pub const IP_OLD_FW_FLUSH: i32 = 52;
/// clear single/all firewall counter(s)
pub const IP_OLD_FW_ZERO: i32 = 53;
/// get entire firewall rule chain
pub const IP_OLD_FW_GET: i32 = 54;
/// set/get NAT opts XXX Deprecated, do not use
pub const IP_NAT__XXX: i32 = 55;
/// reset logging counters
pub const IP_OLD_FW_RESETLOG: i32 = 56;

/// add/configure a dummynet pipe
pub const IP_DUMMYNET_CONFIGURE: i32 = 60;
/// delete a dummynet pipe from chain
pub const IP_DUMMYNET_DEL: i32 = 61;
/// flush dummynet
pub const IP_DUMMYNET_FLUSH: i32 = 62;
/// get entire dummynet pipes
pub const IP_DUMMYNET_GET: i32 = 64;

/// int*; get background IO flags; set background IO
pub const IP_TRAFFIC_MGT_BACKGROUND: i32 = 65;
/// int*; set/get IP multicast i/f index
pub const IP_MULTICAST_IFINDEX: i32 = 66;

/// IPv4 Source Filter Multicast API [RFC3678]
/// join a source-specific group
pub const IP_ADD_SOURCE_MEMBERSHIP: i32 = 70;
/// drop a single source
pub const IP_DROP_SOURCE_MEMBERSHIP: i32 = 71;
/// block a source
pub const IP_BLOCK_SOURCE: i32 = 72;
/// unblock a source
pub const IP_UNBLOCK_SOURCE: i32 = 73;

/// The following option is private; do not use it from user applications.
/// set/get filter list
pub const IP_MSFILTER: i32 = 74;

/// Protocol Independent Multicast API [RFC3678]
/// join an any-source group
pub const MCAST_JOIN_GROUP: i32 = 80;
/// leave all sources for group
pub const MCAST_LEAVE_GROUP: i32 = 81;
/// join a source-specific group
pub const MCAST_JOIN_SOURCE_GROUP: i32 = 82;
/// leave a single source
pub const MCAST_LEAVE_SOURCE_GROUP: i32 = 83;
/// block a source
pub const MCAST_BLOCK_SOURCE: i32 = 84;
/// unblock a source
pub const MCAST_UNBLOCK_SOURCE: i32 = 85;

/// Defaults and limits for options
///
/// normally limit m'casts to 1 hop
pub const IP_DEFAULT_MULTICAST_TTL: i32 = 1;
/// normally hear sends if a member
pub const IP_DEFAULT_MULTICAST_LOOP: i32 = 1;

/// The `imo_membership` vector for each socket is now dynamically allocated at
/// run-time, bounded by `USHRT_MAX`, and is reallocated when needed, sized
/// according to a power-of-two increment.
pub const IP_MIN_MEMBERSHIPS: i32 = 31;
pub const IP_MAX_MEMBERSHIPS: i32 = 4095;

/// Default resource limits for IPv4 multicast source filtering.
/// These may be modified by sysctl.
///
/// sources per group
pub const IP_MAX_GROUP_SRC_FILTER: i32 = 512;
/// sources per socket/group
pub const IP_MAX_SOCK_SRC_FILTER: i32 = 128;
/// XXX no longer used
pub const IP_MAX_SOCK_MUTE_FILTER: i32 = 128;

/// Argument structure for `IP_ADD_MEMBERSHIP` and `IP_DROP_MEMBERSHIP`.
#[repr(C)]
pub struct ip_mreq_t {
    /// IP multicast address of group
    pub imr_multiaddr: in_addr_t,
    /// local IP address of interface
    pub imr_interface: in_addr_t,
}

/// Modified argument structure for `IP_MULTICAST_IF`, obtained from Linux.
///
/// This is used to specify an interface index for multicast sends, as
/// the IPv4 legacy APIs do not support this (unless `IP_SENDIF` is available).
#[repr(C)]
pub struct ip_mreqn_t {
    /// IP multicast address of group
    pub imr_multiaddr: in_addr_t,
    /// local IP address of interface
    pub imr_address: in_addr_t,
    /// Interface index; cast to uint32_t
    pub imr_ifindex: i32,
}

/// Argument structure for IPv4 Multicast Source Filter APIs. [RFC3678]
#[repr(C)]
pub struct ip_mreq_source_t {
    /// IP multicast address of group
    pub imr_multiaddr: in_addr_t,
    /// IP address of source
    pub imr_sourceaddr: in_addr_t,
    /// local IP address of interface
    pub imr_interface: in_addr_t,
}

/// Argument structures for Protocol-Independent Multicast Source
/// Filter APIs. [RFC3678]
#[repr(C)]
pub struct group_req_t {
    /// interface index
    pub gr_interface: u32,
    /// group address
    pub gr_group: sockaddr_storage_t,
}

#[repr(C)]
pub struct group_source_req_t {
    /// interface index
    pub gsr_interface: u32,
    /// group address
    pub gsr_group: sockaddr_storage_t,
    /// source address
    pub gsr_source: sockaddr_storage_t,
}

/// Filter modes; also used to represent per-socket filter mode internally.
///
/// fmode: not yet defined
pub const MCAST_UNDEFINED: i32 = 0;
/// fmode: include these source(s)
pub const MCAST_INCLUDE: i32 = 1;
/// fmode: exclude these source(s)
pub const MCAST_EXCLUDE: i32 = 2;

/// Argument for `IP_PORTRANGE`:
/// - which range to search when port is unspecified at bind() or connect()
///
/// default range
pub const IP_PORTRANGE_DEFAULT: i32 = 0;
/// "high" - request firewall bypass
pub const IP_PORTRANGE_HIGH: i32 = 1;
/// "low" - vouchsafe security
pub const IP_PORTRANGE_LOW: i32 = 2;

/// `IP_PKTINFO`: Packet information (equivalent to  RFC2292 sec 5 for IPv4)
/// This structure is used for
///
/// 1) Receiving ancilliary data about the datagram if `IP_PKTINFO` sockopt is
/// set on the socket. In this case `ipi_ifindex` will contain the interface
/// index the datagram was received on, `ipi_addr` is the IP address the
/// datagram was received to.
///
/// 2) Sending a datagram using a specific interface or IP source address.
/// if `ipi_ifindex` is set to non-zero when `in_pktinfo` is passed as
/// ancilliary data of type `IP_PKTINFO`, this will be used as the source
/// interface to send the datagram from. If `ipi_ifindex` is null, `ip_spec_dst`
/// will be used for the source address.
///
/// Note: if `IP_BOUND_IF` is set on the socket, `ipi_ifindex` in the ancillary
/// `IP_PKTINFO` option silently overrides the bound interface when it is
/// specified during send time.
#[repr(C)]
pub struct in_pktinfo_t {
    /// send/recv interface index
    pub ipi_ifindex: u32,
    /// Local address
    pub ipi_spec_dst: in_addr_t,
    /// IP Header dst address
    pub ipi_addr: in_addr_t,
}

/// Definitions for inet sysctl operations.
///
/// Third level is protocol number.
/// Fourth level is desired variable within that protocol.
///
// don't list to IPPROTO_MAX
pub const IPPROTO_MAXID: i32 = IPPROTO_AH + 1;

/// Names for IP sysctl objects
///
/// act as router
pub const IPCTL_FORWARDING: i32 = 1;
/// may send redirects when forwarding
pub const IPCTL_SENDREDIRECTS: i32 = 2;
/// default TTL
pub const IPCTL_DEFTTL: i32 = 3;
/// default MTU
pub const IPCTL_DEFMTU: i32 = 4;
/// cloned route expiration time
pub const IPCTL_RTEXPIRE: i32 = 5;
/// min value for expiration time
pub const IPCTL_RTMINEXPIRE: i32 = 6;
/// trigger level for dynamic expire
pub const IPCTL_RTMAXCACHE: i32 = 7;
/// may perform source routes
pub const IPCTL_SOURCEROUTE: i32 = 8;
/// may re-broadcast received packets
pub const IPCTL_DIRECTEDBROADCAST: i32 = 9;
/// max length of netisr queue
pub const IPCTL_INTRQMAXLEN: i32 = 10;
/// number of netisr q drops
pub const IPCTL_INTRQDROPS: i32 = 11;
/// ipstat structure
pub const IPCTL_STATS: i32 = 12;
/// may accept source routed packets
pub const IPCTL_ACCEPTSOURCEROUTE: i32 = 13;
/// use fast IP forwarding code
pub const IPCTL_FASTFORWARDING: i32 = 14;
/// deprecated
pub const IPCTL_KEEPFAITH: i32 = 15;
/// default TTL for gif encap packet
pub const IPCTL_GIF_TTL: i32 = 16;
pub const IPCTL_MAXID: i32 = 17;
