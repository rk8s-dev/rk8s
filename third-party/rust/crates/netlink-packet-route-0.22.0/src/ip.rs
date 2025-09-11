// SPDX-License-Identifier: MIT

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use netlink_packet_utils::DecodeError;

pub(crate) const IPV4_ADDR_LEN: usize = 4;
pub(crate) const IPV6_ADDR_LEN: usize = 16;

pub(crate) fn parse_ipv4_addr(raw: &[u8]) -> Result<Ipv4Addr, DecodeError> {
    if raw.len() == IPV4_ADDR_LEN {
        Ok(Ipv4Addr::new(raw[0], raw[1], raw[2], raw[3]))
    } else {
        Err(DecodeError::from(format!(
            "Invalid u8 array length {}, expecting \
            {IPV4_ADDR_LEN} for IPv4 address, got {:?}",
            raw.len(),
            raw,
        )))
    }
}

pub(crate) fn parse_ipv6_addr(raw: &[u8]) -> Result<Ipv6Addr, DecodeError> {
    if raw.len() == IPV6_ADDR_LEN {
        let mut data = [0u8; IPV6_ADDR_LEN];
        data.copy_from_slice(raw);
        Ok(Ipv6Addr::from(data))
    } else {
        Err(DecodeError::from(format!(
            "Invalid u8 array length {}, expecting {IPV6_ADDR_LEN} \
            for IPv6 address, got {:?}",
            raw.len(),
            raw,
        )))
    }
}

pub(crate) fn emit_ip_to_buffer(ip: &IpAddr, buffer: &mut [u8]) {
    match ip {
        IpAddr::V4(ip) => buffer.copy_from_slice(&ip.octets()),
        IpAddr::V6(ip) => buffer.copy_from_slice(&ip.octets()),
    }
}

pub(crate) fn parse_ip_addr(raw: &[u8]) -> Result<IpAddr, DecodeError> {
    if raw.len() == IPV6_ADDR_LEN {
        parse_ipv6_addr(raw).map(IpAddr::from)
    } else if raw.len() == IPV4_ADDR_LEN {
        parse_ipv4_addr(raw).map(IpAddr::from)
    } else {
        Err(DecodeError::from(format!(
            "Invalid u8 array length {}, expecting {IPV6_ADDR_LEN} \
            for IPv6 address or {IPV4_ADDR_LEN} for IPv4 address, got {:?}",
            raw.len(),
            raw,
        )))
    }
}

pub(crate) fn ip_addr_len(addr: &IpAddr) -> usize {
    if addr.is_ipv4() {
        IPV4_ADDR_LEN
    } else {
        IPV6_ADDR_LEN
    }
}

pub(crate) fn emit_ip_addr(addr: &IpAddr, buffer: &mut [u8]) {
    match addr {
        IpAddr::V4(ip) => buffer.copy_from_slice(&ip.octets()),
        IpAddr::V6(ip) => buffer.copy_from_slice(&ip.octets()),
    }
}

// These is defined by Assigned Internet Protocol Numbers, no need to use libc
// as they are supposed to identical between all operating system.
const IPPROTO_HOPOPTS: i32 = 0;
const IPPROTO_ICMP: i32 = 1;
const IPPROTO_IGMP: i32 = 2;
const IPPROTO_IPIP: i32 = 4;
const IPPROTO_TCP: i32 = 6;
const IPPROTO_EGP: i32 = 8;
const IPPROTO_PUP: i32 = 12;
const IPPROTO_UDP: i32 = 17;
const IPPROTO_IDP: i32 = 22;
const IPPROTO_TP: i32 = 29;
const IPPROTO_DCCP: i32 = 33;
const IPPROTO_IPV6: i32 = 41;
const IPPROTO_RSVP: i32 = 46;
const IPPROTO_GRE: i32 = 47;
const IPPROTO_ESP: i32 = 50;
const IPPROTO_AH: i32 = 51;
const IPPROTO_MTP: i32 = 92;
const IPPROTO_BEETPH: i32 = 94;
const IPPROTO_ENCAP: i32 = 98;
const IPPROTO_PIM: i32 = 103;
const IPPROTO_COMP: i32 = 108;
const IPPROTO_L2TP: i32 = 115;
const IPPROTO_SCTP: i32 = 132;
const IPPROTO_UDPLITE: i32 = 136;
const IPPROTO_MPLS: i32 = 137;
const IPPROTO_ETHERNET: i32 = 143;
const IPPROTO_RAW: i32 = 255;
const IPPROTO_MPTCP: i32 = 262;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
#[non_exhaustive]
pub enum IpProtocol {
    Hopopts,
    Icmp,
    Igmp,
    Ipip,
    Tcp,
    Egp,
    Pup,
    Udp,
    Idp,
    Tp,
    Dccp,
    Ipv6,
    Rsvp,
    Gre,
    Esp,
    Ah,
    Mtp,
    Beetph,
    Encap,
    Pim,
    Comp,
    L2tp,
    Sctp,
    Udplite,
    Mpls,
    Ethernet,
    #[default]
    Raw,
    Mptcp,
    Other(i32),
}

impl From<i32> for IpProtocol {
    fn from(d: i32) -> Self {
        match d {
            IPPROTO_HOPOPTS => Self::Hopopts,
            IPPROTO_ICMP => Self::Icmp,
            IPPROTO_IGMP => Self::Igmp,
            IPPROTO_IPIP => Self::Ipip,
            IPPROTO_TCP => Self::Tcp,
            IPPROTO_EGP => Self::Egp,
            IPPROTO_PUP => Self::Pup,
            IPPROTO_UDP => Self::Udp,
            IPPROTO_IDP => Self::Idp,
            IPPROTO_TP => Self::Tp,
            IPPROTO_DCCP => Self::Dccp,
            IPPROTO_IPV6 => Self::Ipv6,
            IPPROTO_RSVP => Self::Rsvp,
            IPPROTO_GRE => Self::Gre,
            IPPROTO_ESP => Self::Esp,
            IPPROTO_AH => Self::Ah,
            IPPROTO_MTP => Self::Mtp,
            IPPROTO_BEETPH => Self::Beetph,
            IPPROTO_ENCAP => Self::Encap,
            IPPROTO_PIM => Self::Pim,
            IPPROTO_COMP => Self::Comp,
            IPPROTO_L2TP => Self::L2tp,
            IPPROTO_SCTP => Self::Sctp,
            IPPROTO_UDPLITE => Self::Udplite,
            IPPROTO_MPLS => Self::Mpls,
            IPPROTO_ETHERNET => Self::Ethernet,
            IPPROTO_RAW => Self::Raw,
            IPPROTO_MPTCP => Self::Mptcp,
            _ => Self::Other(d),
        }
    }
}

impl From<IpProtocol> for i32 {
    fn from(v: IpProtocol) -> i32 {
        match v {
            IpProtocol::Hopopts => IPPROTO_HOPOPTS,
            IpProtocol::Icmp => IPPROTO_ICMP,
            IpProtocol::Igmp => IPPROTO_IGMP,
            IpProtocol::Ipip => IPPROTO_IPIP,
            IpProtocol::Tcp => IPPROTO_TCP,
            IpProtocol::Egp => IPPROTO_EGP,
            IpProtocol::Pup => IPPROTO_PUP,
            IpProtocol::Udp => IPPROTO_UDP,
            IpProtocol::Idp => IPPROTO_IDP,
            IpProtocol::Tp => IPPROTO_TP,
            IpProtocol::Dccp => IPPROTO_DCCP,
            IpProtocol::Ipv6 => IPPROTO_IPV6,
            IpProtocol::Rsvp => IPPROTO_RSVP,
            IpProtocol::Gre => IPPROTO_GRE,
            IpProtocol::Esp => IPPROTO_ESP,
            IpProtocol::Ah => IPPROTO_AH,
            IpProtocol::Mtp => IPPROTO_MTP,
            IpProtocol::Beetph => IPPROTO_BEETPH,
            IpProtocol::Encap => IPPROTO_ENCAP,
            IpProtocol::Pim => IPPROTO_PIM,
            IpProtocol::Comp => IPPROTO_COMP,
            IpProtocol::L2tp => IPPROTO_L2TP,
            IpProtocol::Sctp => IPPROTO_SCTP,
            IpProtocol::Udplite => IPPROTO_UDPLITE,
            IpProtocol::Mpls => IPPROTO_MPLS,
            IpProtocol::Ethernet => IPPROTO_ETHERNET,
            IpProtocol::Raw => IPPROTO_RAW,
            IpProtocol::Mptcp => IPPROTO_MPTCP,
            IpProtocol::Other(d) => d,
        }
    }
}
