// SPDX-License-Identifier: MIT

use futures::stream::StreamExt;

use netlink_packet_core::{
    NetlinkMessage, NetlinkPayload, NLM_F_ACK, NLM_F_CREATE, NLM_F_EXCL,
    NLM_F_REPLACE, NLM_F_REQUEST,
};
use netlink_packet_route::{
    neighbour::{
        NeighbourAddress, NeighbourAttribute, NeighbourFlags, NeighbourMessage,
        NeighbourState,
    },
    route::RouteType,
    AddressFamily, RouteNetlinkMessage,
};

use crate::{Error, Handle};
use std::net::IpAddr;

pub struct NeighbourAddRequest {
    handle: Handle,
    message: NeighbourMessage,
    replace: bool,
}

impl NeighbourAddRequest {
    pub(crate) fn new(handle: Handle, index: u32, destination: IpAddr) -> Self {
        let mut message = NeighbourMessage::default();

        message.header.family = match destination {
            IpAddr::V4(_) => AddressFamily::Inet,
            IpAddr::V6(_) => AddressFamily::Inet6,
        };

        message.header.ifindex = index;
        message.header.state = NeighbourState::Permanent;
        message.header.kind = RouteType::Unspec;

        message.attributes.push(NeighbourAttribute::Destination(
            match destination {
                IpAddr::V4(v4) => NeighbourAddress::Inet(v4),
                IpAddr::V6(v6) => NeighbourAddress::Inet6(v6),
            },
        ));

        NeighbourAddRequest {
            handle,
            message,
            replace: false,
        }
    }

    #[cfg(not(target_os = "freebsd"))]
    pub(crate) fn new_bridge(handle: Handle, index: u32, lla: &[u8]) -> Self {
        let mut message = NeighbourMessage::default();

        message.header.family = AddressFamily::Bridge;
        message.header.ifindex = index;
        message.header.state = NeighbourState::Permanent;
        message.header.kind = RouteType::Unspec;

        message
            .attributes
            .push(NeighbourAttribute::LinkLocalAddress(lla.to_vec()));

        NeighbourAddRequest {
            handle,
            message,
            replace: false,
        }
    }

    /// Set a bitmask of states for the neighbor cache entry.
    /// It should be a combination of `NUD_*` constants.
    pub fn state(mut self, state: NeighbourState) -> Self {
        self.message.header.state = state;
        self
    }

    /// Set flags for the neighbor cache entry.
    /// It should be a combination of `NTF_*` constants.
    pub fn flags(mut self, flags: NeighbourFlags) -> Self {
        self.message.header.flags = flags;
        self
    }

    /// Set attributes applicable to the the neighbor cache entry.
    /// It should be one of `NDA_*` constants.
    pub fn kind(mut self, kind: RouteType) -> Self {
        self.message.header.kind = kind;
        self
    }

    /// Set a neighbor cache link layer address (see `NDA_LLADDR` for details).
    pub fn link_local_address(mut self, addr: &[u8]) -> Self {
        let lla =
            self.message
                .attributes
                .iter_mut()
                .find_map(|nla| match nla {
                    NeighbourAttribute::LinkLocalAddress(lla) => Some(lla),
                    _ => None,
                });

        if let Some(lla) = lla {
            *lla = addr.to_vec();
        } else {
            self.message
                .attributes
                .push(NeighbourAttribute::LinkLocalAddress(addr.to_vec()));
        }

        self
    }

    /// Set the destination address for the neighbour (see `NDA_DST` for
    /// details).
    pub fn destination(mut self, addr: IpAddr) -> Self {
        let dst =
            self.message
                .attributes
                .iter_mut()
                .find_map(|nla| match nla {
                    NeighbourAttribute::Destination(dst) => Some(dst),
                    _ => None,
                });

        let addr = match addr {
            IpAddr::V4(v4) => NeighbourAddress::Inet(v4),
            IpAddr::V6(v6) => NeighbourAddress::Inet6(v6),
        };

        if let Some(dst) = dst {
            *dst = addr;
        } else {
            self.message
                .attributes
                .push(NeighbourAttribute::Destination(addr));
        }

        self
    }

    /// Replace existing matching neighbor.
    pub fn replace(self) -> Self {
        Self {
            replace: true,
            ..self
        }
    }

    /// Execute the request.
    pub async fn execute(self) -> Result<(), Error> {
        let NeighbourAddRequest {
            mut handle,
            message,
            replace,
        } = self;

        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::NewNeighbour(message));
        let replace = if replace { NLM_F_REPLACE } else { NLM_F_EXCL };
        req.header.flags = NLM_F_REQUEST | NLM_F_ACK | replace | NLM_F_CREATE;

        let mut response = handle.request(req)?;
        while let Some(message) = response.next().await {
            if let NetlinkPayload::Error(err) = message.payload {
                return Err(Error::NetlinkError(err));
            }
        }

        Ok(())
    }

    /// Return a mutable reference to the request message.
    pub fn message_mut(&mut self) -> &mut NeighbourMessage {
        &mut self.message
    }
}
