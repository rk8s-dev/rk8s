// SPDX-License-Identifier: MIT

use futures::stream::StreamExt;
use netlink_packet_core::{
    NetlinkMessage, NLM_F_ACK, NLM_F_CREATE, NLM_F_EXCL, NLM_F_REPLACE,
    NLM_F_REQUEST,
};

use netlink_packet_route::{link::LinkMessage, RouteNetlinkMessage};

use crate::{try_nl, Error, Handle};

/// A request to create a new link. This is equivalent to the `ip link add`
/// commands.
///
/// A few methods for common actions (creating a veth pair, creating a vlan
/// interface, etc.) are provided, but custom requests can be made using the
/// [`message_mut()`](#method.message_mut) accessor.
pub struct LinkAddRequest {
    handle: Handle,
    message: LinkMessage,
    flags: u16,
}

impl LinkAddRequest {
    pub(crate) fn new(handle: Handle, message: LinkMessage) -> Self {
        LinkAddRequest {
            handle,
            message,
            flags: NLM_F_REQUEST | NLM_F_ACK | NLM_F_CREATE | NLM_F_EXCL,
        }
    }

    /// Replace existing matching link.
    pub fn replace(self) -> Self {
        let mut ret = self;
        ret.flags -= NLM_F_EXCL;
        ret.flags |= NLM_F_REPLACE;
        ret
    }

    /// Setting arbitrary [NetlinkHeader] flags
    pub fn set_flags(self, flags: u16) -> Self {
        let mut ret = self;
        ret.flags = flags;
        ret
    }

    /// Execute the request.
    pub async fn execute(self) -> Result<(), Error> {
        let LinkAddRequest {
            mut handle,
            message,
            flags,
        } = self;
        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::NewLink(message));
        req.header.flags = flags;

        let mut response = handle.request(req)?;
        while let Some(message) = response.next().await {
            try_nl!(message);
        }
        Ok(())
    }
}
