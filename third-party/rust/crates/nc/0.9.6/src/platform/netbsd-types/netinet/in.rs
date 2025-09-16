// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `/usr/include/netinet/in.h`

#![allow(clippy::module_name_repetitions)]

use crate::{in_addr_t, in_port_t, sa_family_t};

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
/// IP header
pub const IPPROTO_IPV4: i32 = 4;
/// IP inside IP
pub const IPPROTO_IPIP: i32 = 4;
/// tcp
pub const IPPROTO_TCP: i32 = 6;
/// exterior gateway protocol
pub const IPPROTO_EGP: i32 = 8;
/// pup
pub const IPPROTO_PUP: i32 = 12;
/// user datagram protocol
pub const IPPROTO_UDP: i32 = 17;
/// xns idp
pub const IPPROTO_IDP: i32 = 22;
/// tp-4 w/ class negotiation
pub const IPPROTO_TP: i32 = 29;
/// DCCP
pub const IPPROTO_DCCP: i32 = 33;
/// IP6 header
pub const IPPROTO_IPV6: i32 = 41;
/// IP6 routing header
pub const IPPROTO_ROUTING: i32 = 43;
/// IP6 fragmentation header
pub const IPPROTO_FRAGMENT: i32 = 44;
/// resource reservation
pub const IPPROTO_RSVP: i32 = 46;
/// GRE encaps RFC 1701
pub const IPPROTO_GRE: i32 = 47;
/// encap. security payload
pub const IPPROTO_ESP: i32 = 50;
/// authentication header
pub const IPPROTO_AH: i32 = 51;
/// IP Mobility RFC 2004
pub const IPPROTO_MOBILE: i32 = 55;
/// IPv6 ICMP
pub const IPPROTO_IPV6_ICMP: i32 = 58;
/// ICMP6
pub const IPPROTO_ICMPV6: i32 = 58;
/// IP6 no next header
pub const IPPROTO_NONE: i32 = 59;
/// IP6 destination option
pub const IPPROTO_DSTOPTS: i32 = 60;
/// ISO cnlp
pub const IPPROTO_EON: i32 = 80;
/// Ethernet-in-IP
pub const IPPROTO_ETHERIP: i32 = 97;
/// encapsulation header
pub const IPPROTO_ENCAP: i32 = 98;
/// Protocol indep. multicast
pub const IPPROTO_PIM: i32 = 103;
/// IP Payload Comp. Protocol
pub const IPPROTO_IPCOMP: i32 = 108;
/// VRRP RFC 2338
pub const IPPROTO_VRRP: i32 = 112;
/// Common Address Resolution Protocol
pub const IPPROTO_CARP: i32 = 112;
/// `L2TPv3`
pub const IPPROTO_L2TP: i32 = 115;
/// SCTP
pub const IPPROTO_SCTP: i32 = 132;
/// PFSYNC
pub const IPPROTO_PFSYNC: i32 = 240;
/// raw IP packet
pub const IPPROTO_RAW: i32 = 255;
pub const IPPROTO_MAX: i32 = 256;

/// last return value of *_input(), meaning "all job for this pkt is done".
pub const IPPROTO_DONE: i32 = 257;

/// sysctl placeholder for `(FAST_)IPSEC`
pub const CTL_IPPROTO_IPSEC: i32 = 258;

pub const IPPORT_RESERVED: i32 = 1024;
pub const IPPORT_ANONMIN: i32 = 49152;
pub const IPPORT_ANONMAX: i32 = 65535;
pub const IPPORT_RESERVEDMIN: i32 = 600;
pub const IPPORT_RESERVEDMAX: i32 = IPPORT_RESERVED - 1;

/// Internet address (a structure for historical reasons)
#[repr(C)]
pub struct in_addr_s {
    pub s_addr: in_addr_t,
}

#[must_use]
const fn __IPADDR(i: in_addr_t) -> in_addr_t {
    i.to_be()
}

#[must_use]
pub const fn IN_CLASSA(i: in_addr_t) -> bool {
    (i & __IPADDR(0x8000_0000)) == __IPADDR(0x0000_0000)
}
pub const IN_CLASSA_NET: in_addr_t = __IPADDR(0xff00_0000);
pub const IN_CLASSA_NSHIFT: usize = 24;
pub const IN_CLASSA_HOST: in_addr_t = __IPADDR(0x00ff_ffff);
pub const IN_CLASSA_MAX: usize = 128;

#[must_use]
pub const fn IN_CLASSB(i: in_addr_t) -> bool {
    (i & __IPADDR(0xc000_0000)) == __IPADDR(0x8000_0000)
}
pub const IN_CLASSB_NET: in_addr_t = __IPADDR(0xffff_0000);
pub const IN_CLASSB_NSHIFT: usize = 16;
pub const IN_CLASSB_HOST: in_addr_t = __IPADDR(0x0000_ffff);
pub const IN_CLASSB_MAX: usize = 65536;

#[must_use]
pub const fn IN_CLASSC(i: in_addr_t) -> bool {
    (i & __IPADDR(0xe000_0000)) == __IPADDR(0xc000_0000)
}
pub const IN_CLASSC_NET: in_addr_t = __IPADDR(0xffff_ff00);
pub const IN_CLASSC_NSHIFT: usize = 8;
pub const IN_CLASSC_HOST: in_addr_t = __IPADDR(0x0000_00ff);

#[must_use]
pub const fn IN_CLASSD(i: in_addr_t) -> bool {
    (i & __IPADDR(0xf000_0000)) == __IPADDR(0xe000_0000)
}
/// These ones aren't really net and host fields, but routing needn't know.
pub const IN_CLASSD_NET: in_addr_t = __IPADDR(0xf000_0000);
pub const IN_CLASSD_NSHIFT: usize = 28;
pub const IN_CLASSD_HOST: in_addr_t = __IPADDR(0x0fff_ffff);

#[must_use]
#[inline]
pub const fn IN_MULTICAST(i: in_addr_t) -> bool {
    IN_CLASSD(i)
}

#[must_use]
pub const fn IN_EXPERIMENTAL(i: in_addr_t) -> bool {
    (i & __IPADDR(0xf000_0000)) == __IPADDR(0xf000_0000)
}
#[must_use]
pub const fn IN_BADCLASS(i: in_addr_t) -> bool {
    (i & __IPADDR(0xf000_0000)) == __IPADDR(0xf000_0000)
}

#[must_use]
pub const fn IN_LINKLOCAL(i: in_addr_t) -> bool {
    (i & __IPADDR(0xffff_0000)) == __IPADDR(0xa9fe_0000)
}

#[must_use]
pub const fn IN_PRIVATE(i: in_addr_t) -> bool {
    ((i & __IPADDR(0xff00_0000)) == __IPADDR(0x0a00_0000))
        || ((i & __IPADDR(0xfff0_0000)) == __IPADDR(0xac10_0000))
        || ((i & __IPADDR(0xffff_0000)) == __IPADDR(0xc0a8_0000))
}

#[must_use]
pub const fn IN_LOCAL_GROUP(i: in_addr_t) -> bool {
    (i & __IPADDR(0xffff_ff00)) == __IPADDR(0xe000_0000)
}

#[must_use]
pub const fn IN_ANY_LOCAL(i: in_addr_t) -> bool {
    IN_LINKLOCAL(i) || IN_LOCAL_GROUP(i)
}

pub const INADDR_ANY: in_addr_t = __IPADDR(0x0000_0000);
pub const INADDR_LOOPBACK: in_addr_t = __IPADDR(0x7f00_0001);
// must be masked
pub const INADDR_BROADCAST: in_addr_t = __IPADDR(0xffff_ffff);
// -1 return
pub const INADDR_NONE: in_addr_t = __IPADDR(0xffff_ffff);

/// 224.0.0.0
pub const INADDR_UNSPEC_GROUP: in_addr_t = __IPADDR(0xe000_0000);
/// 224.0.0.1
pub const INADDR_ALLHOSTS_GROUP: in_addr_t = __IPADDR(0xe000_0001);
/// 224.0.0.2
pub const INADDR_ALLRTRS_GROUP: in_addr_t = __IPADDR(0xe000_0002);
/// 224.0.0.18
pub const INADDR_CARP_GROUP: in_addr_t = __IPADDR(0xe000_0012);
/// 224.0.0.255
pub const INADDR_MAX_LOCAL_GROUP: in_addr_t = __IPADDR(0xe000_00ff);

/// official!
pub const IN_LOOPBACKNET: in_addr_t = 127;

/// Socket address, internet style.
#[repr(C)]
pub struct sockaddr_in_t {
    pub sin_len: u8,
    pub sin_family: sa_family_t,
    pub sin_port: in_port_t,
    pub sin_addr: in_addr_s,
    sin_zero: [i8; 8],
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
    pub ip_dst: in_addr_s,
    /// actually variable in size
    pub ip_opts: [i8; 40],
}

/// Options for use with [gs]etsockopt at the IP level.
///
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
/// `in_addr`; set/get IP multicast i/f
pub const IP_MULTICAST_IF: i32 = 9;
/// `u_char`; set/get IP multicast ttl
pub const IP_MULTICAST_TTL: i32 = 10;
/// `u_char`; set/get IP multicast loopback
pub const IP_MULTICAST_LOOP: i32 = 11;
/// The add and drop membership option numbers need to match with the v6 ones
/// `ip_mreq`; add an IP group membership
pub const IP_ADD_MEMBERSHIP: i32 = 12;
/// `ip_mreq`; drop an IP group membership
pub const IP_DROP_MEMBERSHIP: i32 = 13;
/// int; port selection algo (rfc6056)
pub const IP_PORTALGO: i32 = 18;
/// int; range to use for ephemeral port
pub const IP_PORTRANGE: i32 = 19;
/// bool; receive reception if w/dgram
pub const IP_RECVIF: i32 = 20;
/// int; get MTU of last xmit = EMSGSIZE
pub const IP_ERRORMTU: i32 = 21;
/// struct; get/set security policy
pub const IP_IPSEC_POLICY: i32 = 22;
/// bool; receive IP TTL w/dgram
pub const IP_RECVTTL: i32 = 23;
/// minimum TTL for packet or drop
pub const IP_MINTTL: i32 = 24;
/// struct; set default src if/addr
pub const IP_PKTINFO: i32 = 25;
/// int; receive dst if/addr w/dgram
pub const IP_RECVPKTINFO: i32 = 26;

/// FreeBSD compatibility
pub const IP_SENDSRCADDR: i32 = IP_RECVDSTADDR;

/// Information sent in the control message of a datagram socket for
/// `IP_PKTINFO` and `IP_RECVPKTINFO`.
#[repr(C)]
pub struct in_pktinfo_t {
    /// src/dst address
    pub ipi_addr: in_addr_s,
    /// interface index
    pub ipi_ifindex: u32,
}

/// Defaults and limits for options
///
/// normally limit m'casts to 1 hop
pub const IP_DEFAULT_MULTICAST_TTL: i32 = 1;
/// normally hear sends if a member
pub const IP_DEFAULT_MULTICAST_LOOP: i32 = 1;
/// per socket; must fit in one mbuf
pub const IP_MAX_MEMBERSHIPS: i32 = 20;

/// Argument structure for `IP_ADD_MEMBERSHIP` and `IP_DROP_MEMBERSHIP`.
#[repr(C)]
pub struct ip_mreq_t {
    /// IP multicast address of group
    pub imr_multiaddr: in_addr_s,
    /// local IP address of interface
    pub imr_interface: in_addr_s,
}

/// Argument for `IP_PORTRANGE`:
/// - which range to search when port is unspecified at bind() or connect()
///
/// default range
pub const IP_PORTRANGE_DEFAULT: i32 = 0;
/// same as DEFAULT (FreeBSD compat)
pub const IP_PORTRANGE_HIGH: i32 = 1;
/// use privileged range
pub const IP_PORTRANGE_LOW: i32 = 2;

/// Names for IP sysctl objects
///
/// act as router
pub const IPCTL_FORWARDING: i32 = 1;
/// may send redirects when forwarding
pub const IPCTL_SENDREDIRECTS: i32 = 2;
/// default TTL
pub const IPCTL_DEFTTL: i32 = 3;
/// `IPCTL_DEFMTU=4`, never implemented
/// forward source-routed packets
pub const IPCTL_FORWSRCRT: i32 = 5;
/// default broadcast behavior
pub const IPCTL_DIRECTEDBCAST: i32 = 6;
/// allow/drop all source-routed pkts
pub const IPCTL_ALLOWSRCRT: i32 = 7;
/// treat subnets as local addresses
pub const IPCTL_SUBNETSARELOCAL: i32 = 8;
/// allow path MTU discovery
pub const IPCTL_MTUDISC: i32 = 9;
/// minimum ephemeral port
pub const IPCTL_ANONPORTMIN: i32 = 10;
/// maximum ephemeral port
pub const IPCTL_ANONPORTMAX: i32 = 11;
/// allow path MTU discovery
pub const IPCTL_MTUDISCTIMEOUT: i32 = 12;
/// maximum ip flows allowed
pub const IPCTL_MAXFLOWS: i32 = 13;
/// is host zero a broadcast addr?
pub const IPCTL_HOSTZEROBROADCAST: i32 = 14;
/// default TTL for gif encap packet
pub const IPCTL_GIF_TTL: i32 = 15;
/// minimum reserved port
pub const IPCTL_LOWPORTMIN: i32 = 16;
/// maximum reserved port
pub const IPCTL_LOWPORTMAX: i32 = 17;
/// max packets reassembly queue
pub const IPCTL_MAXFRAGPACKETS: i32 = 18;
/// default TTL for gre encap packet
pub const IPCTL_GRE_TTL: i32 = 19;
/// drop pkts in from 'wrong' iface
pub const IPCTL_CHECKINTERFACE: i32 = 20;
/// IP packet input queue
pub const IPCTL_IFQ: i32 = 21;
/// use random IP ids (if configured)
pub const IPCTL_RANDOMID: i32 = 22;
/// do IP checksum on loopback
pub const IPCTL_LOOPBACKCKSUM: i32 = 23;
/// IP statistics
pub const IPCTL_STATS: i32 = 24;
/// DAD packets to send
pub const IPCTL_DAD_COUNT: i32 = 25;
