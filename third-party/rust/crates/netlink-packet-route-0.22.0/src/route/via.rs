// SPDX-License-Identifier: MIT

use std::net::{Ipv4Addr, Ipv6Addr};

use netlink_packet_utils::{
    traits::{Emitable, Parseable},
    DecodeError,
};

use crate::ip::{
    parse_ipv4_addr, parse_ipv6_addr, IPV4_ADDR_LEN, IPV6_ADDR_LEN,
};
use crate::AddressFamily;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
// Kernel representative is `struct rtvia`
// In Linux kernel 6.18, MPLS route also use `AF_PACKET`
// and MPLS route. Even the MPLS is using AF_PACKET, so we cannot simply
// treat `RouteVia` as `IpAddr`.
pub enum RouteVia {
    Inet(Ipv4Addr),
    Inet6(Ipv6Addr),
    #[cfg(any(target_os = "linux", target_os = "fuchsia"))]
    Packet(Vec<u8>),
    Other((AddressFamily, Vec<u8>)),
}

const RTVIA_LEN: usize = 2;

buffer!(RouteViaBuffer(RTVIA_LEN) {
    address_family: (u16, 0..2),
    address: (slice, RTVIA_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<RouteViaBuffer<&'a T>>
    for RouteVia
{
    fn parse(buf: &RouteViaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let address_family: AddressFamily = (buf.address_family() as u8).into();
        Ok(match address_family {
            AddressFamily::Inet => Self::Inet(parse_ipv4_addr(buf.address())?),
            AddressFamily::Inet6 => {
                Self::Inet6(parse_ipv6_addr(buf.address())?)
            }
            #[cfg(any(target_os = "linux", target_os = "fuchsia"))]
            AddressFamily::Packet => Self::Packet(buf.address().to_vec()),
            _ => Self::Other((address_family, buf.address().to_vec())),
        })
    }
}

impl Emitable for RouteVia {
    fn buffer_len(&self) -> usize {
        match self {
            Self::Inet(_) => IPV4_ADDR_LEN + 2,
            Self::Inet6(_) => IPV6_ADDR_LEN + 2,
            #[cfg(any(target_os = "linux", target_os = "fuchsia"))]
            Self::Packet(a) => a.len() + 2,
            Self::Other((_, a)) => a.len() + 2,
        }
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = RouteViaBuffer::new(buffer);
        let (address_family, addr) = match self {
            Self::Inet(ip) => (AddressFamily::Inet, ip.octets().to_vec()),
            Self::Inet6(ip) => (AddressFamily::Inet6, ip.octets().to_vec()),
            #[cfg(any(target_os = "linux", target_os = "fuchsia"))]
            Self::Packet(a) => (AddressFamily::Packet, a.to_vec()),
            Self::Other((f, a)) => (*f, a.to_vec()),
        };
        buffer.set_address_family(u8::from(address_family).into());
        buffer.address_mut().copy_from_slice(addr.as_slice());
    }
}

impl From<Ipv4Addr> for RouteVia {
    fn from(v: Ipv4Addr) -> Self {
        Self::Inet(v)
    }
}

impl From<Ipv6Addr> for RouteVia {
    fn from(v: Ipv6Addr) -> Self {
        Self::Inet6(v)
    }
}
