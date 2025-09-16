// SPDX-License-Identifier: MIT

use futures::Stream;
use netlink_packet_core::NetlinkMessage;
use netlink_packet_route::RouteNetlinkMessage;
use netlink_proto::{sys::SocketAddr, ConnectionHandle};

use crate::{
    AddressHandle, Error, LinkHandle, NeighbourHandle, RouteHandle, RuleHandle,
};
#[cfg(not(target_os = "freebsd"))]
use crate::{
    QDiscHandle, TrafficChainHandle, TrafficClassHandle, TrafficFilterHandle,
};

#[derive(Clone, Debug)]
pub struct Handle(ConnectionHandle<RouteNetlinkMessage>);

impl Handle {
    pub(crate) fn new(conn: ConnectionHandle<RouteNetlinkMessage>) -> Self {
        Handle(conn)
    }

    pub fn request(
        &mut self,
        message: NetlinkMessage<RouteNetlinkMessage>,
    ) -> Result<impl Stream<Item = NetlinkMessage<RouteNetlinkMessage>>, Error>
    {
        self.0
            .request(message, SocketAddr::new(0, 0))
            .map_err(|_| Error::RequestFailed)
    }

    pub fn notify(
        &mut self,
        msg: NetlinkMessage<RouteNetlinkMessage>,
    ) -> Result<(), Error> {
        self.0
            .notify(msg, SocketAddr::new(0, 0))
            .map_err(|_| Error::RequestFailed)?;
        Ok(())
    }

    /// Create a new handle, specifically for link requests (equivalent to `ip
    /// link` commands)
    pub fn link(&self) -> LinkHandle {
        LinkHandle::new(self.clone())
    }

    /// Create a new handle, specifically for address requests (equivalent to
    /// `ip addr` commands)
    pub fn address(&self) -> AddressHandle {
        AddressHandle::new(self.clone())
    }

    /// Create a new handle, specifically for routing table requests (equivalent
    /// to `ip route` commands)
    pub fn route(&self) -> RouteHandle {
        RouteHandle::new(self.clone())
    }

    /// Create a new handle, specifically for routing rule requests (equivalent
    /// to `ip rule` commands)
    pub fn rule(&self) -> RuleHandle {
        RuleHandle::new(self.clone())
    }

    /// Create a new handle, specifically for routing neighbours requests
    /// (equivalent to `ip neighbour` commands)
    pub fn neighbours(&self) -> NeighbourHandle {
        NeighbourHandle::new(self.clone())
    }

    /// Create a new handle, specifically for traffic control qdisc requests
    /// (equivalent to `tc qdisc show` commands)
    #[cfg(not(target_os = "freebsd"))]
    pub fn qdisc(&self) -> QDiscHandle {
        QDiscHandle::new(self.clone())
    }

    /// Create a new handle, specifically for traffic control class requests
    /// (equivalent to `tc class show dev <interface_name>` commands)
    #[cfg(not(target_os = "freebsd"))]
    pub fn traffic_class(&self, ifindex: i32) -> TrafficClassHandle {
        TrafficClassHandle::new(self.clone(), ifindex)
    }

    /// Create a new handle, specifically for traffic control filter requests
    /// (equivalent to `tc filter show dev <interface_name>` commands)
    #[cfg(not(target_os = "freebsd"))]
    pub fn traffic_filter(&self, ifindex: i32) -> TrafficFilterHandle {
        TrafficFilterHandle::new(self.clone(), ifindex)
    }

    /// Create a new handle, specifically for traffic control chain requests
    /// (equivalent to `tc chain show dev <interface_name>` commands)
    #[cfg(not(target_os = "freebsd"))]
    pub fn traffic_chain(&self, ifindex: i32) -> TrafficChainHandle {
        TrafficChainHandle::new(self.clone(), ifindex)
    }
}
