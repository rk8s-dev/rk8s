// SPDX-License-Identifier: MIT

use futures::stream::StreamExt;
use std::net::{IpAddr, Ipv4Addr};

use netlink_packet_core::{
    NetlinkMessage, NLM_F_ACK, NLM_F_CREATE, NLM_F_EXCL, NLM_F_REPLACE,
    NLM_F_REQUEST,
};

use netlink_packet_route::{
    address::{AddressAttribute, AddressMessage},
    AddressFamily, RouteNetlinkMessage,
};

use crate::{try_nl, Error, Handle};

/// A request to create a new address. This is equivalent to the `ip address
/// add` commands.
pub struct AddressAddRequest {
    handle: Handle,
    message: AddressMessage,
    replace: bool,
}

impl AddressAddRequest {
    pub(crate) fn new(
        handle: Handle,
        index: u32,
        address: IpAddr,
        prefix_len: u8,
    ) -> Self {
        let mut message = AddressMessage::default();

        message.header.prefix_len = prefix_len;
        message.header.index = index;

        message.header.family = match address {
            IpAddr::V4(_) => AddressFamily::Inet,
            IpAddr::V6(_) => AddressFamily::Inet6,
        };

        if address.is_multicast() {
            if let IpAddr::V6(a) = address {
                message.attributes.push(AddressAttribute::Multicast(a));
            }
        } else {
            message.attributes.push(AddressAttribute::Address(address));

            // for IPv4 the IFA_LOCAL address can be set to the same value as
            // IFA_ADDRESS
            message.attributes.push(AddressAttribute::Local(address));

            // set the IFA_BROADCAST address as well (IPv6 does not support
            // broadcast)
            if let IpAddr::V4(a) = address {
                if prefix_len == 32 {
                    message.attributes.push(AddressAttribute::Broadcast(a));
                } else {
                    let ip_addr = u32::from(a);
                    let brd = Ipv4Addr::from(
                        (0xffff_ffff_u32) >> u32::from(prefix_len) | ip_addr,
                    );
                    message.attributes.push(AddressAttribute::Broadcast(brd));
                };
            }
        }
        AddressAddRequest {
            handle,
            message,
            replace: false,
        }
    }

    /// Replace existing matching address.
    pub fn replace(self) -> Self {
        Self {
            replace: true,
            ..self
        }
    }

    /// Execute the request.
    pub async fn execute(self) -> Result<(), Error> {
        let AddressAddRequest {
            mut handle,
            message,
            replace,
        } = self;
        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::NewAddress(message));
        let replace = if replace { NLM_F_REPLACE } else { NLM_F_EXCL };
        req.header.flags = NLM_F_REQUEST | NLM_F_ACK | replace | NLM_F_CREATE;

        let mut response = handle.request(req)?;
        while let Some(message) = response.next().await {
            try_nl!(message);
        }
        Ok(())
    }

    /// Return a mutable reference to the request message.
    pub fn message_mut(&mut self) -> &mut AddressMessage {
        &mut self.message
    }
}
