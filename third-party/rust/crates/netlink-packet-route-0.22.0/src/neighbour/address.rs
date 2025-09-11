// SPDX-License-Identifier: MIT

use std::net::{Ipv4Addr, Ipv6Addr};

use netlink_packet_utils::{DecodeError, Emitable};

use crate::{
    ip::{
        emit_ip_to_buffer, parse_ipv4_addr, parse_ipv6_addr, IPV4_ADDR_LEN,
        IPV6_ADDR_LEN,
    },
    AddressFamily,
};

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum NeighbourAddress {
    Inet(Ipv4Addr),
    Inet6(Ipv6Addr),
    Other(Vec<u8>),
}

impl NeighbourAddress {
    pub(crate) fn parse_with_param(
        address_family: AddressFamily,
        payload: &[u8],
    ) -> Result<Self, DecodeError> {
        Ok(match address_family {
            AddressFamily::Inet => Self::Inet(parse_ipv4_addr(payload)?),
            AddressFamily::Inet6 => Self::Inet6(parse_ipv6_addr(payload)?),
            _ => Self::Other(payload.to_vec()),
        })
    }
}

impl Emitable for NeighbourAddress {
    fn buffer_len(&self) -> usize {
        match self {
            Self::Inet(_) => IPV4_ADDR_LEN,
            Self::Inet6(_) => IPV6_ADDR_LEN,
            Self::Other(v) => v.len(),
        }
    }

    fn emit(&self, buffer: &mut [u8]) {
        match self {
            Self::Inet(v) => emit_ip_to_buffer(&((*v).into()), buffer),
            Self::Inet6(v) => emit_ip_to_buffer(&((*v).into()), buffer),
            Self::Other(v) => buffer.copy_from_slice(v.as_slice()),
        }
    }
}

impl From<Ipv4Addr> for NeighbourAddress {
    fn from(v: Ipv4Addr) -> Self {
        Self::Inet(v)
    }
}

impl From<Ipv6Addr> for NeighbourAddress {
    fn from(v: Ipv6Addr) -> Self {
        Self::Inet6(v)
    }
}
