// SPDX-License-Identifier: MIT

use crate::{Handle, RouteAddRequest, RouteDelRequest, RouteGetRequest};
use netlink_packet_route::route::RouteMessage;

#[derive(Debug, Clone)]
pub struct RouteHandle(Handle);

impl RouteHandle {
    pub fn new(handle: Handle) -> Self {
        RouteHandle(handle)
    }

    /// Retrieve the list of routing table entries (equivalent to `ip route
    /// show`)
    /// The `RouteMessage` could be built by [crate::RouteMessageBuilder].
    /// In order to perform kernel side filter, please enable
    /// `NETLINK_GET_STRICT_CHK` via
    /// `rtnetlink::sys::Socket::set_netlink_get_strict_chk(true)`.
    pub fn get(&self, route: RouteMessage) -> RouteGetRequest {
        RouteGetRequest::new(self.0.clone(), route)
    }

    /// Add an routing table entry (equivalent to `ip route add`)
    /// The `RouteMessage` could be built by [crate::RouteMessageBuilder].
    pub fn add(&self, route: RouteMessage) -> RouteAddRequest {
        RouteAddRequest::new(self.0.clone(), route)
    }

    /// Delete the given routing table entry (equivalent to `ip route del`)
    /// The `RouteMessage` could be built by [crate::RouteMessageBuilder].
    pub fn del(&self, route: RouteMessage) -> RouteDelRequest {
        RouteDelRequest::new(self.0.clone(), route)
    }
}
