// SPDX-License-Identifier: MIT

use std::{
    marker::PhantomData,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use netlink_packet_route::{
    route::{
        RouteAddress, RouteAttribute, RouteHeader, RouteMessage, RouteProtocol,
        RouteScope, RouteType,
    },
    AddressFamily,
};

#[derive(Debug, Clone)]
pub struct RouteMessageBuilder<T = IpAddr> {
    message: RouteMessage,
    _phantom: PhantomData<T>,
}

impl<T> RouteMessageBuilder<T> {
    /// Create default RouteMessage with header set to:
    ///  * route: [RouteHeader::RT_TABLE_MAIN]
    ///  * protocol: [RouteProtocol::Static]
    ///  * scope: [RouteScope::Universe]
    ///  * kind: [RouteType::Unicast]
    ///
    /// For using this message in querying routes, these settings
    /// are ignored unless `NETLINK_GET_STRICT_CHK` been enabled.
    fn new_no_address_family() -> Self {
        let mut message = RouteMessage::default();
        message.header.table = RouteHeader::RT_TABLE_MAIN;
        message.header.protocol = RouteProtocol::Static;
        message.header.scope = RouteScope::Universe;
        message.header.kind = RouteType::Unicast;
        Self {
            message,
            _phantom: Default::default(),
        }
    }

    /// Sets the input interface index.
    pub fn input_interface(mut self, index: u32) -> Self {
        self.message.attributes.push(RouteAttribute::Iif(index));
        self
    }

    /// Sets the output interface index.
    pub fn output_interface(mut self, index: u32) -> Self {
        self.message.attributes.push(RouteAttribute::Oif(index));
        self
    }

    /// Sets the route priority (metric)
    pub fn priority(mut self, priority: u32) -> Self {
        self.message
            .attributes
            .push(RouteAttribute::Priority(priority));
        self
    }

    /// Sets the route table ID.
    ///
    /// Default is main route table.
    pub fn table_id(mut self, table: u32) -> Self {
        if table > 255 {
            self.message.attributes.push(RouteAttribute::Table(table));
        } else {
            self.message.header.table = table as u8;
        }
        self
    }

    /// Sets the route protocol.
    ///
    /// Default is static route protocol.
    pub fn protocol(mut self, protocol: RouteProtocol) -> Self {
        self.message.header.protocol = protocol;
        self
    }

    /// Sets the route scope.
    ///
    /// Default is universe route scope.
    pub fn scope(mut self, scope: RouteScope) -> Self {
        self.message.header.scope = scope;
        self
    }

    /// Sets the route kind.
    ///
    /// Default is unicast route kind.
    pub fn kind(mut self, kind: RouteType) -> Self {
        self.message.header.kind = kind;
        self
    }

    /// Return a mutable reference to the request message.
    pub fn get_mut(&mut self) -> &mut RouteMessage {
        &mut self.message
    }

    pub fn build(self) -> RouteMessage {
        self.message
    }
}

impl RouteMessageBuilder<Ipv4Addr> {
    /// Create default RouteMessage with header set to:
    ///  * route: [RouteHeader::RT_TABLE_MAIN]
    ///  * protocol: [RouteProtocol::Static]
    ///  * scope: [RouteScope::Universe]
    ///  * kind: [RouteType::Unicast]
    ///  * address_family: [AddressFamily::Inet4]
    ///
    /// For using this message in querying routes, these settings
    /// are ignored unless `NETLINK_GET_STRICT_CHK` been enabled.
    pub fn new() -> Self {
        let mut builder = Self::new_no_address_family();
        builder.get_mut().header.address_family = AddressFamily::Inet;
        builder
    }

    /// Sets the source address prefix.
    pub fn source_prefix(mut self, addr: Ipv4Addr, prefix_length: u8) -> Self {
        self.message.header.source_prefix_length = prefix_length;
        self.message
            .attributes
            .push(RouteAttribute::Source(RouteAddress::Inet(addr)));
        self
    }

    /// Sets the preferred source address.
    pub fn pref_source(mut self, addr: Ipv4Addr) -> Self {
        self.message
            .attributes
            .push(RouteAttribute::PrefSource(RouteAddress::Inet(addr)));
        self
    }

    /// Sets the destination address prefix.
    pub fn destination_prefix(
        mut self,
        addr: Ipv4Addr,
        prefix_length: u8,
    ) -> Self {
        self.message.header.destination_prefix_length = prefix_length;
        self.message
            .attributes
            .push(RouteAttribute::Destination(RouteAddress::Inet(addr)));
        self
    }

    /// Sets the gateway (via) address.
    pub fn gateway(mut self, addr: Ipv4Addr) -> Self {
        self.message
            .attributes
            .push(RouteAttribute::Gateway(RouteAddress::Inet(addr)));
        self
    }
}

impl Default for RouteMessageBuilder<Ipv4Addr> {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteMessageBuilder<Ipv6Addr> {
    /// Create default RouteMessage with header set to:
    ///  * route: [RouteHeader::RT_TABLE_MAIN]
    ///  * protocol: [RouteProtocol::Static]
    ///  * scope: [RouteScope::Universe]
    ///  * kind: [RouteType::Unicast]
    ///  * address_family: [AddressFamily::Inet6]
    ///
    /// For using this message in querying routes, these settings
    /// are ignored unless `NETLINK_GET_STRICT_CHK` been enabled.
    pub fn new() -> Self {
        let mut builder = Self::new_no_address_family();
        builder.get_mut().header.address_family = AddressFamily::Inet6;
        builder
    }

    /// Sets the source address prefix.
    pub fn source_prefix(mut self, addr: Ipv6Addr, prefix_length: u8) -> Self {
        self.message.header.source_prefix_length = prefix_length;
        self.message
            .attributes
            .push(RouteAttribute::Source(RouteAddress::Inet6(addr)));
        self
    }

    /// Sets the preferred source address.
    pub fn pref_source(mut self, addr: Ipv6Addr) -> Self {
        self.message
            .attributes
            .push(RouteAttribute::PrefSource(RouteAddress::Inet6(addr)));
        self
    }

    /// Sets the destination address prefix.
    pub fn destination_prefix(
        mut self,
        addr: Ipv6Addr,
        prefix_length: u8,
    ) -> Self {
        self.message.header.destination_prefix_length = prefix_length;
        self.message
            .attributes
            .push(RouteAttribute::Destination(RouteAddress::Inet6(addr)));
        self
    }

    /// Sets the gateway (via) address.
    pub fn gateway(mut self, addr: Ipv6Addr) -> Self {
        self.message
            .attributes
            .push(RouteAttribute::Gateway(RouteAddress::Inet6(addr)));
        self
    }
}

impl Default for RouteMessageBuilder<Ipv6Addr> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidRouteMessage {
    #[error("invalid address family {:?}", _0)]
    AddressFamily(AddressFamily),

    #[error("invalid gateway {}", _0)]
    Gateway(IpAddr),

    #[error("invalid preferred source {}", _0)]
    PrefSource(IpAddr),

    #[error("invalid source prefix {}/{}", _0, _1)]
    SourcePrefix(IpAddr, u8),

    #[error("invalid destination prefix {}/{}", _0, _1)]
    DestinationPrefix(IpAddr, u8),
}

impl RouteMessageBuilder<IpAddr> {
    pub fn new() -> Self {
        Self::new_no_address_family()
    }

    /// Sets the source address prefix.
    pub fn source_prefix(
        mut self,
        addr: IpAddr,
        prefix_length: u8,
    ) -> Result<Self, InvalidRouteMessage> {
        self.set_address_family_from_ip_addr(addr);
        match self.message.header.address_family {
            AddressFamily::Inet => {
                if addr.is_ipv6() || prefix_length > 32 {
                    return Err(InvalidRouteMessage::SourcePrefix(
                        addr,
                        prefix_length,
                    ));
                }
            }
            AddressFamily::Inet6 => {
                if addr.is_ipv4() || prefix_length > 128 {
                    return Err(InvalidRouteMessage::SourcePrefix(
                        addr,
                        prefix_length,
                    ));
                }
            }
            af => return Err(InvalidRouteMessage::AddressFamily(af)),
        };
        self.message
            .attributes
            .push(RouteAttribute::Source(addr.into()));
        self.message.header.source_prefix_length = prefix_length;
        Ok(self)
    }

    /// Sets the preferred source address.
    pub fn pref_source(
        mut self,
        addr: IpAddr,
    ) -> Result<Self, InvalidRouteMessage> {
        self.set_address_family_from_ip_addr(addr);
        match self.message.header.address_family {
            AddressFamily::Inet => {
                if addr.is_ipv6() {
                    return Err(InvalidRouteMessage::PrefSource(addr));
                };
            }
            AddressFamily::Inet6 => {
                if addr.is_ipv4() {
                    return Err(InvalidRouteMessage::PrefSource(addr));
                };
            }
            af => {
                return Err(InvalidRouteMessage::AddressFamily(af));
            }
        }
        self.message
            .attributes
            .push(RouteAttribute::PrefSource(addr.into()));
        Ok(self)
    }

    /// Sets the destination address prefix.
    pub fn destination_prefix(
        mut self,
        addr: IpAddr,
        prefix_length: u8,
    ) -> Result<Self, InvalidRouteMessage> {
        self.set_address_family_from_ip_addr(addr);
        match self.message.header.address_family {
            AddressFamily::Inet => {
                if addr.is_ipv6() || prefix_length > 32 {
                    return Err(InvalidRouteMessage::DestinationPrefix(
                        addr,
                        prefix_length,
                    ));
                }
            }
            AddressFamily::Inet6 => {
                if addr.is_ipv4() || prefix_length > 128 {
                    return Err(InvalidRouteMessage::DestinationPrefix(
                        addr,
                        prefix_length,
                    ));
                }
            }
            af => {
                return Err(InvalidRouteMessage::AddressFamily(af));
            }
        };
        self.message.header.destination_prefix_length = prefix_length;
        self.message
            .attributes
            .push(RouteAttribute::Destination(addr.into()));
        Ok(self)
    }

    /// Sets the gateway (via) address.
    pub fn gateway(
        mut self,
        addr: IpAddr,
    ) -> Result<Self, InvalidRouteMessage> {
        self.set_address_family_from_ip_addr(addr);
        match self.message.header.address_family {
            AddressFamily::Inet => {
                if addr.is_ipv6() {
                    return Err(InvalidRouteMessage::Gateway(addr));
                };
            }
            AddressFamily::Inet6 => {
                if addr.is_ipv4() {
                    return Err(InvalidRouteMessage::Gateway(addr));
                };
            }
            af => {
                return Err(InvalidRouteMessage::AddressFamily(af));
            }
        }
        self.message
            .attributes
            .push(RouteAttribute::Gateway(addr.into()));
        Ok(self)
    }

    /// If it is not set already, set the address family based on the
    /// given IP address. This is a noop is the address family is
    /// already set.
    fn set_address_family_from_ip_addr(&mut self, addr: IpAddr) {
        if self.message.header.address_family != AddressFamily::Unspec {
            return;
        }
        if addr.is_ipv4() {
            self.message.header.address_family = AddressFamily::Inet;
        } else {
            self.message.header.address_family = AddressFamily::Inet6;
        }
    }
}

impl Default for RouteMessageBuilder<IpAddr> {
    fn default() -> Self {
        Self::new()
    }
}
