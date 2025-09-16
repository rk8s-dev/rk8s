// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    nla::{NlaBuffer, NlasIterator},
    traits::{Emitable, Parseable},
    DecodeError,
};

use super::{super::AddressFamily, flags::RouteFlags};

const ROUTE_HEADER_LEN: usize = 12;

buffer!(RouteMessageBuffer(ROUTE_HEADER_LEN) {
    address_family: (u8, 0),
    destination_prefix_length: (u8, 1),
    source_prefix_length: (u8, 2),
    tos: (u8, 3),
    table: (u8, 4),
    protocol: (u8, 5),
    scope: (u8, 6),
    kind: (u8, 7),
    flags: (u32, 8..ROUTE_HEADER_LEN),
    payload: (slice, ROUTE_HEADER_LEN..),
});

impl<'a, T: AsRef<[u8]> + ?Sized> RouteMessageBuffer<&'a T> {
    pub fn attributes(
        &self,
    ) -> impl Iterator<Item = Result<NlaBuffer<&'a [u8]>, DecodeError>> {
        NlasIterator::new(self.payload())
    }
}

/// High level representation of `RTM_GETROUTE`, `RTM_ADDROUTE`, `RTM_DELROUTE`
/// messages headers.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct RouteHeader {
    /// Address family of the route: either [AddressFamily::Inet] for IPv4,
    /// or [AddressFamily::Inet6] for IPv6.
    pub address_family: AddressFamily,
    /// Prefix length of the destination subnet.
    pub destination_prefix_length: u8,
    /// Prefix length of the source address.
    pub source_prefix_length: u8,
    /// Type of service.
    pub tos: u8,
    /// Routing table ID.
    pub table: u8,
    /// Route Protocol
    pub protocol: RouteProtocol,
    /// Route scope
    pub scope: RouteScope,
    /// Route type.
    pub kind: RouteType,
    /// Flags when querying the kernel with a `RTM_GETROUTE` message.
    pub flags: RouteFlags,
}

impl RouteHeader {
    pub const RT_TABLE_MAIN: u8 = 254;
    pub const RT_TABLE_UNSPEC: u8 = 0;
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<RouteMessageBuffer<&'a T>>
    for RouteHeader
{
    fn parse(buf: &RouteMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        Ok(RouteHeader {
            address_family: buf.address_family().into(),
            destination_prefix_length: buf.destination_prefix_length(),
            source_prefix_length: buf.source_prefix_length(),
            tos: buf.tos(),
            table: buf.table(),
            protocol: buf.protocol().into(),
            scope: buf.scope().into(),
            kind: buf.kind().into(),
            flags: RouteFlags::from_bits_retain(buf.flags()),
        })
    }
}

impl Emitable for RouteHeader {
    fn buffer_len(&self) -> usize {
        ROUTE_HEADER_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = RouteMessageBuffer::new(buffer);
        buffer.set_address_family(self.address_family.into());
        buffer.set_destination_prefix_length(self.destination_prefix_length);
        buffer.set_source_prefix_length(self.source_prefix_length);
        buffer.set_tos(self.tos);
        buffer.set_table(self.table);
        buffer.set_protocol(self.protocol.into());
        buffer.set_scope(self.scope.into());
        buffer.set_kind(self.kind.into());
        buffer.set_flags(self.flags.bits());
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum RouteProtocol {
    Unspec,
    IcmpRedirect,
    Kernel,
    Boot,
    Static,
    Gated,
    Ra,
    Mrt,
    Zebra,
    Bird,
    DnRouted,
    Xorp,
    Ntk,
    Dhcp,
    Mrouted,
    KeepAlived,
    Babel,
    Bgp,
    Isis,
    Ospf,
    Rip,
    Eigrp,
    Other(u8),
}

const RTPROT_UNSPEC: u8 = 0;
const RTPROT_REDIRECT: u8 = 1;
const RTPROT_KERNEL: u8 = 2;
const RTPROT_BOOT: u8 = 3;
const RTPROT_STATIC: u8 = 4;
const RTPROT_GATED: u8 = 8;
const RTPROT_RA: u8 = 9;
const RTPROT_MRT: u8 = 10;
const RTPROT_ZEBRA: u8 = 11;
const RTPROT_BIRD: u8 = 12;
const RTPROT_DNROUTED: u8 = 13;
const RTPROT_XORP: u8 = 14;
const RTPROT_NTK: u8 = 15;
const RTPROT_DHCP: u8 = 16;
const RTPROT_MROUTED: u8 = 17;
const RTPROT_KEEPALIVED: u8 = 18;
const RTPROT_BABEL: u8 = 42;
const RTPROT_BGP: u8 = 186;
const RTPROT_ISIS: u8 = 187;
const RTPROT_OSPF: u8 = 188;
const RTPROT_RIP: u8 = 189;
const RTPROT_EIGRP: u8 = 192;

impl From<RouteProtocol> for u8 {
    fn from(t: RouteProtocol) -> u8 {
        match t {
            RouteProtocol::Unspec => RTPROT_UNSPEC,
            RouteProtocol::IcmpRedirect => RTPROT_REDIRECT,
            RouteProtocol::Kernel => RTPROT_KERNEL,
            RouteProtocol::Boot => RTPROT_BOOT,
            RouteProtocol::Static => RTPROT_STATIC,
            RouteProtocol::Gated => RTPROT_GATED,
            RouteProtocol::Ra => RTPROT_RA,
            RouteProtocol::Mrt => RTPROT_MRT,
            RouteProtocol::Zebra => RTPROT_ZEBRA,
            RouteProtocol::Bird => RTPROT_BIRD,
            RouteProtocol::DnRouted => RTPROT_DNROUTED,
            RouteProtocol::Xorp => RTPROT_XORP,
            RouteProtocol::Ntk => RTPROT_NTK,
            RouteProtocol::Dhcp => RTPROT_DHCP,
            RouteProtocol::Mrouted => RTPROT_MROUTED,
            RouteProtocol::KeepAlived => RTPROT_KEEPALIVED,
            RouteProtocol::Babel => RTPROT_BABEL,
            RouteProtocol::Bgp => RTPROT_BGP,
            RouteProtocol::Isis => RTPROT_ISIS,
            RouteProtocol::Ospf => RTPROT_OSPF,
            RouteProtocol::Rip => RTPROT_RIP,
            RouteProtocol::Eigrp => RTPROT_EIGRP,
            RouteProtocol::Other(d) => d,
        }
    }
}

impl From<u8> for RouteProtocol {
    fn from(d: u8) -> Self {
        match d {
            RTPROT_UNSPEC => RouteProtocol::Unspec,
            RTPROT_REDIRECT => RouteProtocol::IcmpRedirect,
            RTPROT_KERNEL => RouteProtocol::Kernel,
            RTPROT_BOOT => RouteProtocol::Boot,
            RTPROT_STATIC => RouteProtocol::Static,
            RTPROT_GATED => RouteProtocol::Gated,
            RTPROT_RA => RouteProtocol::Ra,
            RTPROT_MRT => RouteProtocol::Mrt,
            RTPROT_ZEBRA => RouteProtocol::Zebra,
            RTPROT_BIRD => RouteProtocol::Bird,
            RTPROT_DNROUTED => RouteProtocol::DnRouted,
            RTPROT_XORP => RouteProtocol::Xorp,
            RTPROT_NTK => RouteProtocol::Ntk,
            RTPROT_DHCP => RouteProtocol::Dhcp,
            RTPROT_MROUTED => RouteProtocol::Mrouted,
            RTPROT_KEEPALIVED => RouteProtocol::KeepAlived,
            RTPROT_BABEL => RouteProtocol::Babel,
            RTPROT_BGP => RouteProtocol::Bgp,
            RTPROT_ISIS => RouteProtocol::Isis,
            RTPROT_OSPF => RouteProtocol::Ospf,
            RTPROT_RIP => RouteProtocol::Rip,
            RTPROT_EIGRP => RouteProtocol::Eigrp,
            _ => RouteProtocol::Other(d),
        }
    }
}

impl std::fmt::Display for RouteProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unspec => write!(f, "unspec"),
            Self::IcmpRedirect => write!(f, "icmp_redirect"),
            Self::Kernel => write!(f, "kernel"),
            Self::Boot => write!(f, "boot"),
            Self::Static => write!(f, "static"),
            Self::Gated => write!(f, "gated"),
            Self::Ra => write!(f, "ra"),
            Self::Mrt => write!(f, "merit_mrt"),
            Self::Zebra => write!(f, "zebra"),
            Self::Bird => write!(f, "bird"),
            Self::DnRouted => write!(f, "decnet_routing_daemon"),
            Self::Xorp => write!(f, "xorp"),
            Self::Ntk => write!(f, "netsukuku"),
            Self::Dhcp => write!(f, "Dhcp"),
            Self::Mrouted => write!(f, "multicast_daemon"),
            Self::KeepAlived => write!(f, "keepalived_daemon"),
            Self::Babel => write!(f, "babel"),
            Self::Bgp => write!(f, "bgp"),
            Self::Isis => write!(f, "isis"),
            Self::Ospf => write!(f, "ospf"),
            Self::Rip => write!(f, "rip"),
            Self::Eigrp => write!(f, "eigrp"),
            Self::Other(v) => write!(f, "other({v})"),
        }
    }
}

impl Default for RouteProtocol {
    fn default() -> Self {
        Self::Unspec
    }
}

impl Parseable<[u8]> for RouteProtocol {
    fn parse(buf: &[u8]) -> Result<Self, DecodeError> {
        if buf.len() == 1 {
            Ok(Self::from(buf[0]))
        } else {
            Err(DecodeError::from(format!(
                "Expecting single u8 for route protocol, but got {:?}",
                buf
            )))
        }
    }
}

impl Emitable for RouteProtocol {
    fn buffer_len(&self) -> usize {
        1
    }

    fn emit(&self, buffer: &mut [u8]) {
        buffer[0] = u8::from(*self);
    }
}

const RT_SCOPE_UNIVERSE: u8 = 0;
const RT_SCOPE_SITE: u8 = 200;
const RT_SCOPE_LINK: u8 = 253;
const RT_SCOPE_HOST: u8 = 254;
const RT_SCOPE_NOWHERE: u8 = 255;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum RouteScope {
    Universe,
    Site,
    Link,
    Host,
    NoWhere,
    Other(u8),
}

impl From<RouteScope> for u8 {
    fn from(v: RouteScope) -> Self {
        match v {
            RouteScope::Universe => RT_SCOPE_UNIVERSE,
            RouteScope::Site => RT_SCOPE_SITE,
            RouteScope::Link => RT_SCOPE_LINK,
            RouteScope::Host => RT_SCOPE_HOST,
            RouteScope::NoWhere => RT_SCOPE_NOWHERE,
            RouteScope::Other(s) => s,
        }
    }
}

impl From<u8> for RouteScope {
    fn from(d: u8) -> Self {
        match d {
            RT_SCOPE_UNIVERSE => RouteScope::Universe,
            RT_SCOPE_SITE => RouteScope::Site,
            RT_SCOPE_LINK => RouteScope::Link,
            RT_SCOPE_HOST => RouteScope::Host,
            RT_SCOPE_NOWHERE => RouteScope::NoWhere,
            _ => RouteScope::Other(d),
        }
    }
}

impl Default for RouteScope {
    fn default() -> Self {
        Self::Universe
    }
}

impl std::fmt::Display for RouteScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Universe => write!(f, "universe"),
            Self::Site => write!(f, "site"),
            Self::Link => write!(f, "link"),
            Self::Host => write!(f, "host"),
            Self::NoWhere => write!(f, "no_where"),
            Self::Other(s) => write!(f, "other({s})"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum RouteType {
    /// Unknown
    Unspec,
    /// Gateway or direct route
    Unicast,
    /// Accept locally
    Local,
    /// Accept locally as broadcast, send as broadcast
    Broadcast,
    /// Accept locally as broadcast, but send as unicast
    Anycast,
    /// Multicast route
    Multicast,
    /// Drop
    BlackHole,
    /// Destination is unreachable
    Unreachable,
    /// Administratively prohibited
    Prohibit,
    /// Not in this table
    Throw,
    /// Translate this address
    Nat,
    /// Use external resolver
    ExternalResolve,
    Other(u8),
}

const RTN_UNSPEC: u8 = 0;
const RTN_UNICAST: u8 = 1;
const RTN_LOCAL: u8 = 2;
const RTN_BROADCAST: u8 = 3;
const RTN_ANYCAST: u8 = 4;
const RTN_MULTICAST: u8 = 5;
const RTN_BLACKHOLE: u8 = 6;
const RTN_UNREACHABLE: u8 = 7;
const RTN_PROHIBIT: u8 = 8;
const RTN_THROW: u8 = 9;
const RTN_NAT: u8 = 10;
const RTN_XRESOLVE: u8 = 11;

impl From<u8> for RouteType {
    fn from(d: u8) -> Self {
        match d {
            RTN_UNSPEC => Self::Unspec,
            RTN_UNICAST => Self::Unicast,
            RTN_LOCAL => Self::Local,
            RTN_BROADCAST => Self::Broadcast,
            RTN_ANYCAST => Self::Anycast,
            RTN_MULTICAST => Self::Multicast,
            RTN_BLACKHOLE => Self::BlackHole,
            RTN_UNREACHABLE => Self::Unreachable,
            RTN_PROHIBIT => Self::Prohibit,
            RTN_THROW => Self::Throw,
            RTN_NAT => Self::Nat,
            RTN_XRESOLVE => Self::ExternalResolve,
            _ => Self::Other(d),
        }
    }
}

impl Default for RouteType {
    fn default() -> Self {
        Self::Unspec
    }
}

impl From<RouteType> for u8 {
    fn from(v: RouteType) -> Self {
        match v {
            RouteType::Unspec => RTN_UNSPEC,
            RouteType::Unicast => RTN_UNICAST,
            RouteType::Local => RTN_LOCAL,
            RouteType::Broadcast => RTN_BROADCAST,
            RouteType::Anycast => RTN_ANYCAST,
            RouteType::Multicast => RTN_MULTICAST,
            RouteType::BlackHole => RTN_BLACKHOLE,
            RouteType::Unreachable => RTN_UNREACHABLE,
            RouteType::Prohibit => RTN_PROHIBIT,
            RouteType::Throw => RTN_THROW,
            RouteType::Nat => RTN_NAT,
            RouteType::ExternalResolve => RTN_XRESOLVE,
            RouteType::Other(d) => d,
        }
    }
}
