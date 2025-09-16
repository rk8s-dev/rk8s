// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/in6.h`
//!
//! Types and definitions for `AF_INET6`

#![allow(clippy::module_name_repetitions)]
#![allow(clippy::uninlined_format_args)]

use core::fmt;

use crate::{be16_t, be32_t};

/// IPv6 address structure
#[repr(C)]
#[derive(Clone, Copy)]
pub union in6_un_t {
    pub u6_addr8: [u8; 16],
    pub u6_addr16: [be16_t; 8],
    pub u6_addr32: [be32_t; 4],
}

impl fmt::Debug for in6_un_t {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let val = unsafe { self.u6_addr8 };
        write!(f, "in6_un_t: {:?}", val)
    }
}

impl Default for in6_un_t {
    fn default() -> Self {
        Self {
            u6_addr8: [0_u8; 16],
        }
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct in6_addr_t {
    pub in6_u: in6_un_t,
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct sockaddr_in6_t {
    /// `AF_INET6`
    pub sin6_family: u16,
    /// Transport layer port
    pub sin6_port: be16_t,
    /// IPv6 flow information
    pub sin6_flowinfo: be32_t,
    /// IPv6 address
    pub sin6_addr: in6_addr_t,
    /// scope id (new in RFC2553)
    pub sin6_scope_id: u32,
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct ipv6_mreq_t {
    /// IPv6 multicast address of group
    pub ipv6mr_multiaddr: in6_addr_t,

    /// local IPv6 address of interface
    pub ipv6mr_ifindex: i32,
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct in6_flowlabel_req_t {
    pub flr_dst: in6_addr_t,
    pub flr_label: be32_t,
    pub flr_action: u8,
    pub flr_share: u8,
    pub flr_flags: u16,
    pub flr_expires: u16,
    pub flr_linger: u16,
    flr_pad: u32,
    // Options in format of IPV6_PKTOPTIONS
}

pub const IPV6_FL_A_GET: i32 = 0;
pub const IPV6_FL_A_PUT: i32 = 1;
pub const IPV6_FL_A_RENEW: i32 = 2;

pub const IPV6_FL_F_CREATE: i32 = 1;
pub const IPV6_FL_F_EXCL: i32 = 2;
pub const IPV6_FL_F_REFLECT: i32 = 4;
pub const IPV6_FL_F_REMOTE: i32 = 8;

pub const IPV6_FL_S_NONE: i32 = 0;
pub const IPV6_FL_S_EXCL: i32 = 1;
pub const IPV6_FL_S_PROCESS: i32 = 2;
pub const IPV6_FL_S_USER: i32 = 3;
pub const IPV6_FL_S_ANY: i32 = 255;

/// Bitmask constant declarations to help applications select out the
/// flow label and priority fields.
///
/// Note that this are in host byte order while the flowinfo field of
/// `sockaddr_in6` is in network byte order.
pub const IPV6_FLOWINFO_FLOWLABEL: i32 = 0x000f_ffff;
pub const IPV6_FLOWINFO_PRIORITY: i32 = 0x0ff0_0000;

/// These definitions are obsolete
pub const IPV6_PRIORITY_UNCHARACTERIZED: i32 = 0x0000;
pub const IPV6_PRIORITY_FILLER: i32 = 0x0100;
pub const IPV6_PRIORITY_UNATTENDED: i32 = 0x0200;
pub const IPV6_PRIORITY_RESERVED1: i32 = 0x0300;
pub const IPV6_PRIORITY_BULK: i32 = 0x0400;
pub const IPV6_PRIORITY_RESERVED2: i32 = 0x0500;
pub const IPV6_PRIORITY_INTERACTIVE: i32 = 0x0600;
pub const IPV6_PRIORITY_CONTROL: i32 = 0x0700;
pub const IPV6_PRIORITY_8: i32 = 0x0800;
pub const IPV6_PRIORITY_9: i32 = 0x0900;
pub const IPV6_PRIORITY_10: i32 = 0x0a00;
pub const IPV6_PRIORITY_11: i32 = 0x0b00;
pub const IPV6_PRIORITY_12: i32 = 0x0c00;
pub const IPV6_PRIORITY_13: i32 = 0x0d00;
pub const IPV6_PRIORITY_14: i32 = 0x0e00;
pub const IPV6_PRIORITY_15: i32 = 0x0f00;

/// IPV6 extension headers
/// IPv6 hop-by-hop options
pub const IPPROTO_HOPOPTS: i32 = 0;
/// IPv6 routing header
pub const IPPROTO_ROUTING: i32 = 43;
/// IPv6 fragmentation header
pub const IPPROTO_FRAGMENT: i32 = 44;
/// `ICMPv6`
pub const IPPROTO_ICMPV6: i32 = 58;
/// IPv6 no next header
pub const IPPROTO_NONE: i32 = 59;
/// IPv6 destination options
pub const IPPROTO_DSTOPTS: i32 = 60;
/// IPv6 mobility header
pub const IPPROTO_MH: i32 = 135;

/// IPv6 TLV options.
pub const IPV6_TLV_PAD1: i32 = 0;
pub const IPV6_TLV_PADN: i32 = 1;
pub const IPV6_TLV_ROUTERALERT: i32 = 5;
/// RFC 5570
pub const IPV6_TLV_CALIPSO: i32 = 7;
pub const IPV6_TLV_JUMBO: i32 = 194;
/// home address option
pub const IPV6_TLV_HAO: i32 = 201;

/// IPV6 socket options
pub const IPV6_ADDRFORM: i32 = 1;
pub const IPV6_2292PKTINFO: i32 = 2;
pub const IPV6_2292HOPOPTS: i32 = 3;
pub const IPV6_2292DSTOPTS: i32 = 4;
pub const IPV6_2292RTHDR: i32 = 5;
pub const IPV6_2292PKTOPTIONS: i32 = 6;
pub const IPV6_CHECKSUM: i32 = 7;
pub const IPV6_2292HOPLIMIT: i32 = 8;
pub const IPV6_NEXTHOP: i32 = 9;
/// obsolete
pub const IPV6_AUTHHDR: i32 = 10;
pub const IPV6_FLOWINFO: i32 = 11;

pub const IPV6_UNICAST_HOPS: i32 = 16;
pub const IPV6_MULTICAST_IF: i32 = 17;
pub const IPV6_MULTICAST_HOPS: i32 = 18;
pub const IPV6_MULTICAST_LOOP: i32 = 19;
pub const IPV6_ADD_MEMBERSHIP: i32 = 20;
pub const IPV6_DROP_MEMBERSHIP: i32 = 21;
pub const IPV6_ROUTER_ALERT: i32 = 22;
pub const IPV6_MTU_DISCOVER: i32 = 23;
pub const IPV6_MTU: i32 = 24;
pub const IPV6_RECVERR: i32 = 25;
pub const IPV6_V6ONLY: i32 = 26;
pub const IPV6_JOIN_ANYCAST: i32 = 27;
pub const IPV6_LEAVE_ANYCAST: i32 = 28;
pub const IPV6_MULTICAST_ALL: i32 = 29;
pub const IPV6_ROUTER_ALERT_ISOLATE: i32 = 30;

/// `IPV6_MTU_DISCOVER` values
pub const IPV6_PMTUDISC_DONT: i32 = 0;
pub const IPV6_PMTUDISC_WANT: i32 = 1;
pub const IPV6_PMTUDISC_DO: i32 = 2;
pub const IPV6_PMTUDISC_PROBE: i32 = 3;
/// same as `IPV6_PMTUDISC_PROBE`, provided for symetry with IPv4
/// also see comments on `IP_PMTUDISC_INTERFACE`
pub const IPV6_PMTUDISC_INTERFACE: i32 = 4;
/// weaker version of `IPV6_PMTUDISC_INTERFACE`, which allows packets to
/// get fragmented if they exceed the interface mtu
pub const IPV6_PMTUDISC_OMIT: i32 = 5;

/// Flowlabel
pub const IPV6_FLOWLABEL_MGR: i32 = 32;
pub const IPV6_FLOWINFO_SEND: i32 = 33;

pub const IPV6_IPSEC_POLICY: i32 = 34;
pub const IPV6_XFRM_POLICY: i32 = 35;
pub const IPV6_HDRINCL: i32 = 36;

/*
 * Multicast:
 * Following socket options are shared between IPv4 and IPv6.
 *
 * MCAST_JOIN_GROUP		42
 * MCAST_BLOCK_SOURCE		43
 * MCAST_UNBLOCK_SOURCE		44
 * MCAST_LEAVE_GROUP		45
 * MCAST_JOIN_SOURCE_GROUP	46
 * MCAST_LEAVE_SOURCE_GROUP	47
 * MCAST_MSFILTER		48
 */

/// Advanced API (RFC3542) (1)
/// Note: `IPV6_RECVRTHDRDSTOPTS` does not exist.
pub const IPV6_RECVPKTINFO: i32 = 49;
pub const IPV6_PKTINFO: i32 = 50;
pub const IPV6_RECVHOPLIMIT: i32 = 51;
pub const IPV6_HOPLIMIT: i32 = 52;
pub const IPV6_RECVHOPOPTS: i32 = 53;
pub const IPV6_HOPOPTS: i32 = 54;
pub const IPV6_RTHDRDSTOPTS: i32 = 55;
pub const IPV6_RECVRTHDR: i32 = 56;
pub const IPV6_RTHDR: i32 = 57;
pub const IPV6_RECVDSTOPTS: i32 = 58;
pub const IPV6_DSTOPTS: i32 = 59;
pub const IPV6_RECVPATHMTU: i32 = 60;
pub const IPV6_PATHMTU: i32 = 61;
pub const IPV6_DONTFRAG: i32 = 62;
//pub const IPV6_USE_MIN_MTU: i32 = 63;

/*
 * Netfilter (1)
 *
 * Following socket options are used in ip6_tables;
 * see include/linux/netfilter_ipv6/ip6_tables.h.
 *
 * IP6T_SO_SET_REPLACE / IP6T_SO_GET_INFO		64
 * IP6T_SO_SET_ADD_COUNTERS / IP6T_SO_GET_ENTRIES	65
 */

/// Advanced API (RFC3542) (2)
pub const IPV6_RECVTCLASS: i32 = 66;
pub const IPV6_TCLASS: i32 = 67;

/*
 * Netfilter (2)
 *
 * Following socket options are used in ip6_tables;
 * see include/linux/netfilter_ipv6/ip6_tables.h.
 *
 * IP6T_SO_GET_REVISION_MATCH	68
 * IP6T_SO_GET_REVISION_TARGET	69
 * IP6T_SO_ORIGINAL_DST		80
 */

pub const IPV6_AUTOFLOWLABEL: i32 = 70;
/// RFC5014: Source address selection
pub const IPV6_ADDR_PREFERENCES: i32 = 72;

pub const IPV6_PREFER_SRC_TMP: i32 = 0x0001;
pub const IPV6_PREFER_SRC_PUBLIC: i32 = 0x0002;
pub const IPV6_PREFER_SRC_PUBTMP_DEFAULT: i32 = 0x0100;
pub const IPV6_PREFER_SRC_COA: i32 = 0x0004;
pub const IPV6_PREFER_SRC_HOME: i32 = 0x0400;
pub const IPV6_PREFER_SRC_CGA: i32 = 0x0008;
pub const IPV6_PREFER_SRC_NONCGA: i32 = 0x0800;

/// RFC5082: Generalized Ttl Security Mechanism
pub const IPV6_MINHOPCOUNT: i32 = 73;

pub const IPV6_ORIGDSTADDR: i32 = 74;
pub const IPV6_RECVORIGDSTADDR: i32 = IPV6_ORIGDSTADDR;
pub const IPV6_TRANSPARENT: i32 = 75;
pub const IPV6_UNICAST_IF: i32 = 76;
pub const IPV6_RECVFRAGSIZE: i32 = 77;
pub const IPV6_FREEBIND: i32 = 78;

/*
 * Multicast Routing:
 * see include/uapi/linux/mroute6.h.
 *
 * MRT6_BASE			200
 * ...
 * MRT6_MAX
 */
