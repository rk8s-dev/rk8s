// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/in.h`

#![allow(overflowing_literals)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::module_name_repetitions)]

use core::mem::size_of;

use crate::{be16_t, be32_t, sa_family_t, sockaddr_storage_t};

/// INET: An implementation of the TCP/IP protocol suite for the LINUX
/// operating system. INET is implemented using the  BSD Socket
/// interface as the means of communication with the user level.
///
/// Definitions of the Internet Protocol.
///
/// Standard well-defined IP protocols.
/// Dummy protocol for TCP
pub const IPPROTO_IP: i32 = 0;
/// Internet Control Message Protocol
pub const IPPROTO_ICMP: i32 = 1;
/// Internet Group Management Protocol
pub const IPPROTO_IGMP: i32 = 2;
/// IPIP tunnels (older KA9Q tunnels use 94)
pub const IPPROTO_IPIP: i32 = 4;
/// Transmission Control Protocol
pub const IPPROTO_TCP: i32 = 6;
/// Exterior Gateway Protocol
pub const IPPROTO_EGP: i32 = 8;
/// PUP protocol
pub const IPPROTO_PUP: i32 = 12;
/// User Datagram Protocol
pub const IPPROTO_UDP: i32 = 17;
/// XNS IDP protocol
pub const IPPROTO_IDP: i32 = 22;
/// SO Transport Protocol Class 4
pub const IPPROTO_TP: i32 = 29;
/// Datagram Congestion Control Protocol
pub const IPPROTO_DCCP: i32 = 33;
/// IPv6-in-IPv4 tunnelling
pub const IPPROTO_IPV6: i32 = 41;
/// RSVP Protocol
pub const IPPROTO_RSVP: i32 = 46;
/// Cisco GRE tunnels (rfc 1701,1702)
pub const IPPROTO_GRE: i32 = 47;
/// Encapsulation Security Payload protocol
pub const IPPROTO_ESP: i32 = 50;
/// Authentication Header protocol
pub const IPPROTO_AH: i32 = 51;
/// Multicast Transport Protocol
pub const IPPROTO_MTP: i32 = 92;
/// IP option pseudo header for BEET
pub const IPPROTO_BEETPH: i32 = 94;
/// Encapsulation Header
pub const IPPROTO_ENCAP: i32 = 98;
/// Protocol Independent Multicast
pub const IPPROTO_PIM: i32 = 103;
/// Compression Header Protocol
pub const IPPROTO_COMP: i32 = 108;
/// Stream Control Transport Protocol
pub const IPPROTO_SCTP: i32 = 132;
/// UDP-Lite (RFC 3828)
pub const IPPROTO_UDPLITE: i32 = 136;
/// MPLS in IP (RFC 4023)
pub const IPPROTO_MPLS: i32 = 137;
/// Raw IP packets
pub const IPPROTO_RAW: i32 = 255;
pub const IPPROTO_MAX: i32 = IPPROTO_RAW + 1;

/// Internet address.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct in_addr_t {
    pub s_addr: be32_t,
}

pub const IP_TOS: i32 = 1;
pub const IP_TTL: i32 = 2;
pub const IP_HDRINCL: i32 = 3;
pub const IP_OPTIONS: i32 = 4;
pub const IP_ROUTER_ALERT: i32 = 5;
pub const IP_RECVOPTS: i32 = 6;
pub const IP_RETOPTS: i32 = 7;
pub const IP_PKTINFO: i32 = 8;
pub const IP_PKTOPTIONS: i32 = 9;
pub const IP_MTU_DISCOVER: i32 = 10;
pub const IP_RECVERR: i32 = 11;
pub const IP_RECVTTL: i32 = 12;
pub const IP_RECVTOS: i32 = 13;
pub const IP_MTU: i32 = 14;
pub const IP_FREEBIND: i32 = 15;
pub const IP_IPSEC_POLICY: i32 = 16;
pub const IP_XFRM_POLICY: i32 = 17;
pub const IP_PASSSEC: i32 = 18;
pub const IP_TRANSPARENT: i32 = 19;

/// BSD compatibility
pub const IP_RECVRETOPTS: i32 = IP_RETOPTS;

/// Proxy original addresses
pub const IP_ORIGDSTADDR: i32 = 20;
pub const IP_RECVORIGDSTADDR: i32 = IP_ORIGDSTADDR;

pub const IP_MINTTL: i32 = 21;
pub const IP_NODEFRAG: i32 = 22;
pub const IP_CHECKSUM: i32 = 23;
pub const IP_BIND_ADDRESS_NO_PORT: i32 = 24;
pub const IP_RECVFRAGSIZE: i32 = 25;

/// `IP_MTU_DISCOVER` values
/// Never send DF frames
pub const IP_PMTUDISC_DONT: i32 = 0;
/// Use per route hints
pub const IP_PMTUDISC_WANT: i32 = 1;
/// Always DF
pub const IP_PMTUDISC_DO: i32 = 2;
/// Ignore dst pmtu
pub const IP_PMTUDISC_PROBE: i32 = 3;
/// Always use interface mtu (ignores dst pmtu) but don't set DF flag.
/// Also incoming ICMP `frag_needed` notifications will be ignored on
/// this socket to prevent accepting spoofed ones.
pub const IP_PMTUDISC_INTERFACE: i32 = 4;
// weaker version of IP_PMTUDISC_INTERFACE, which allos packets to get
// fragmented if they exeed the interface mtu
pub const IP_PMTUDISC_OMIT: i32 = 5;

pub const IP_MULTICAST_IF: i32 = 32;
pub const IP_MULTICAST_TTL: i32 = 33;
pub const IP_MULTICAST_LOOP: i32 = 34;
pub const IP_ADD_MEMBERSHIP: i32 = 35;
pub const IP_DROP_MEMBERSHIP: i32 = 36;
pub const IP_UNBLOCK_SOURCE: i32 = 37;
pub const IP_BLOCK_SOURCE: i32 = 38;
pub const IP_ADD_SOURCE_MEMBERSHIP: i32 = 39;
pub const IP_DROP_SOURCE_MEMBERSHIP: i32 = 40;
pub const IP_MSFILTER: i32 = 41;
pub const MCAST_JOIN_GROUP: i32 = 42;
pub const MCAST_BLOCK_SOURCE: i32 = 43;
pub const MCAST_UNBLOCK_SOURCE: i32 = 44;
pub const MCAST_LEAVE_GROUP: i32 = 45;
pub const MCAST_JOIN_SOURCE_GROUP: i32 = 46;
pub const MCAST_LEAVE_SOURCE_GROUP: i32 = 47;
pub const MCAST_MSFILTER: i32 = 48;
pub const IP_MULTICAST_ALL: i32 = 49;
pub const IP_UNICAST_IF: i32 = 50;

pub const MCAST_EXCLUDE: i32 = 0;
pub const MCAST_INCLUDE: i32 = 1;

/// These need to appear somewhere around here
pub const IP_DEFAULT_MULTICAST_TTL: i32 = 1;
pub const IP_DEFAULT_MULTICAST_LOOP: i32 = 1;

/// Request struct for multicast socket ops
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ip_mreq_t {
    /// IP multicast address of group
    pub imr_multiaddr: in_addr_t,
    /// local IP address of interface
    pub imr_interface: in_addr_t,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ip_mreqn_t {
    /// IP multicast address of group
    pub imr_multiaddr: in_addr_t,
    /// local IP address of interface
    pub imr_address: in_addr_t,
    /// Interface index
    pub imr_ifindex: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ip_mreq_source_t {
    pub imr_multiaddr: be32_t,
    pub imr_interface: be32_t,
    pub imr_sourceaddr: be32_t,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ip_msfilter_t {
    pub imsf_multiaddr: be32_t,
    pub imsf_interface: be32_t,
    pub imsf_fmode: u32,
    pub imsf_numsrc: u32,
    pub imsf_slist: [be32_t; 1],
}

//#define IP_MSFILTER_SIZE(numsrc) \
//	(sizeof(struct ip_msfilter) - sizeof(__u32) \
//	+ (numsrc) * sizeof(__u32))

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct group_req_t {
    /// interface index
    pub gr_interface: u32,
    /// group address
    pub gr_group: sockaddr_storage_t,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct group_source_req_t {
    /// interface index
    pub gsr_interface: u32,
    /// group address
    pub gsr_group: sockaddr_storage_t,
    /// source address
    pub gsr_source: sockaddr_storage_t,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct group_filter_t {
    /// interface index
    pub gf_interface: u32,
    /// multicast address
    pub gf_group: sockaddr_storage_t,
    /// filter mode
    pub gf_fmode: u32,
    /// number of sources
    pub gf_numsrc: u32,
    /* interface index */
    pub gf_slist: [sockaddr_storage_t; 1],
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct in_pktinfo_t {
    pub ipi_ifindex: i32,
    pub ipi_spec_dst: in_addr_t,
    pub ipi_addr: in_addr_t,
}

/// Structure describing an Internet (IP) socket address.
/// sizeof(struct sockaddr)
pub const SOCK_SIZE: usize = 16;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sockaddr_in_t {
    /// Address family
    pub sin_family: sa_family_t,
    /// Port number
    pub sin_port: be16_t,
    /// Internet address
    pub sin_addr: in_addr_t,

    /// Pad to size of `struct sockaddr`.
    pub pad: [u8; SOCK_SIZE - size_of::<i16>() - size_of::<u16>() - size_of::<in_addr_t>()],
}

/// Definitions of the bits in an Internet address integer.
/// On subnets, host and network parts are found according
/// to the subnet mask, not these masks.
#[inline]
#[must_use]
pub const fn in_class_a(a: i32) -> bool {
    ((a as u32) & 0x8000_0000) == 0
}
pub const IN_CLASSA_NET: i32 = 0xff00_0000;
pub const IN_CLASSA_NSHIFT: i32 = 24;
pub const IN_CLASSA_HOST: i32 = !IN_CLASSA_NET;
pub const IN_CLASSA_MAX: i32 = 128;

#[inline]
#[must_use]
pub const fn in_class_b(a: i32) -> bool {
    ((a as u32) & 0xc000_0000) == 0x8000_0000
}

pub const IN_CLASSB_NET: i32 = 0xffff_0000;
pub const IN_CLASSB_NSHIFT: i32 = 16;
pub const IN_CLASSB_HOST: i32 = !IN_CLASSB_NET;
pub const IN_CLASSB_MAX: i32 = 65536;

#[inline]
#[must_use]
pub const fn in_class_c(a: i32) -> bool {
    ((a as u32) & 0xe000_0000) == 0xc000_0000
}

pub const IN_CLASSC_NET: i32 = 0xffff_ff00;
pub const IN_CLASSC_NSHIFT: i32 = 8;
pub const IN_CLASSC_HOST: i32 = !IN_CLASSC_NET;

#[inline]
#[must_use]
pub const fn in_class_d(a: i32) -> bool {
    ((a as u32) & 0xf000_0000) == 0xe000_0000
}

#[inline]
#[must_use]
pub const fn in_multicast(a: i32) -> bool {
    in_class_d(a)
}

pub const IN_MULTICAST_NET: i32 = 0xe000_0000;

#[inline]
#[must_use]
pub const fn in_badclass(a: i32) -> bool {
    (a as u32) == 0xffff_ffff
}

#[inline]
#[must_use]
pub const fn in_experimental(a: i32) -> bool {
    in_badclass(a)
}

#[inline]
#[must_use]
pub const fn in_class_e(a: i32) -> bool {
    ((a as u32) & 0xf000_0000) == 0xf000_0000
}

pub const IN_CLASSE_NET: i32 = 0xffff_ffff;
pub const IN_CLASSE_NSHIFT: i32 = 0;

/// Address to accept any incoming messages.
pub const INADDR_ANY: i32 = 0x0000_0000;

/// Address to send to all hosts.
pub const INADDR_BROADCAST: i32 = 0xffff_ffff;

/// Address indicating an error return.
pub const INADDR_NONE: i32 = 0xffff_ffff;

/// Network number for local host loopback.
pub const IN_LOOPBACKNET: i32 = 127;

/// Address to loopback in software to local host.
/// 127.0.0.1
pub const INADDR_LOOPBACK: i32 = 0x7f00_0001;

#[inline]
#[must_use]
pub const fn in_loopback(a: i32) -> bool {
    (a & 0xff00_0000) == 0x7f00_0000
}

/// Defines for Multicast INADDR
/// 224.0.0.0
pub const INADDR_UNSPEC_GROUP: i32 = 0xe000_0000;
/// 224.0.0.1
pub const INADDR_ALLHOSTS_GROUP: i32 = 0xe000_0001;
/// 224.0.0.2
pub const INADDR_ALLRTRS_GROUP: i32 = 0xe000_0002;
/// 224.0.0.106
pub const INADDR_ALLSNOOPERS_GROUP: i32 = 0xe000_006a;
/// 224.0.0.255
pub const INADDR_MAX_LOCAL_GROUP: i32 = 0xe000_00ff;
