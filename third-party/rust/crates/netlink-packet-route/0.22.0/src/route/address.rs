// SPDX-License-Identifier: MIT

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use netlink_packet_utils::{DecodeError, Emitable};

use crate::{
    ip::{
        emit_ip_to_buffer, parse_ipv4_addr, parse_ipv6_addr, IPV4_ADDR_LEN,
        IPV6_ADDR_LEN,
    },
    route::MplsLabel,
    AddressFamily,
};

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum RouteAddress {
    Inet(Ipv4Addr),
    Inet6(Ipv6Addr),
    Mpls(MplsLabel),
    Other(Vec<u8>),
}

impl From<IpAddr> for RouteAddress {
    fn from(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(ipv4) => Self::Inet(ipv4),
            IpAddr::V6(ipv6) => Self::Inet6(ipv6),
        }
    }
}

impl RouteAddress {
    pub(crate) fn parse(
        address_family: AddressFamily,
        payload: &[u8],
    ) -> Result<Self, DecodeError> {
        Ok(match address_family {
            AddressFamily::Inet => Self::Inet(parse_ipv4_addr(payload)?),
            AddressFamily::Inet6 => Self::Inet6(parse_ipv6_addr(payload)?),
            #[cfg(any(target_os = "linux", target_os = "fuchsia"))]
            AddressFamily::Mpls => Self::Mpls(MplsLabel::parse(payload)?),
            _ => Self::Other(payload.to_vec()),
        })
    }
}

impl Emitable for RouteAddress {
    fn buffer_len(&self) -> usize {
        match self {
            Self::Inet(_) => IPV4_ADDR_LEN,
            Self::Inet6(_) => IPV6_ADDR_LEN,
            Self::Mpls(v) => v.buffer_len(),
            Self::Other(v) => v.len(),
        }
    }

    fn emit(&self, buffer: &mut [u8]) {
        match self {
            Self::Inet(v) => emit_ip_to_buffer(&((*v).into()), buffer),
            Self::Inet6(v) => emit_ip_to_buffer(&((*v).into()), buffer),
            Self::Mpls(v) => v.emit(buffer),
            Self::Other(v) => buffer.copy_from_slice(v.as_slice()),
        }
    }
}

impl From<Ipv4Addr> for RouteAddress {
    fn from(v: Ipv4Addr) -> Self {
        Self::Inet(v)
    }
}

impl From<Ipv6Addr> for RouteAddress {
    fn from(v: Ipv6Addr) -> Self {
        Self::Inet6(v)
    }
}

impl From<MplsLabel> for RouteAddress {
    fn from(v: MplsLabel) -> Self {
        Self::Mpls(v)
    }
}
