// SPDX-License-Identifier: MIT

use futures::stream::StreamExt;
use netlink_packet_core::{
    NetlinkMessage, NLM_F_ACK, NLM_F_CREATE, NLM_F_EXCL, NLM_F_REQUEST,
};
use netlink_packet_route::{link::LinkMessage, RouteNetlinkMessage};

use crate::{try_nl, Error, Handle};

pub struct LinkSetRequest {
    handle: Handle,
    message: LinkMessage,
}

impl LinkSetRequest {
    pub(crate) fn new(handle: Handle, message: LinkMessage) -> Self {
        LinkSetRequest { handle, message }
    }

    /// Execute the request
    pub async fn execute(self) -> Result<(), Error> {
        let LinkSetRequest {
            mut handle,
            message,
        } = self;
        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::SetLink(message));
        req.header.flags =
            NLM_F_REQUEST | NLM_F_ACK | NLM_F_EXCL | NLM_F_CREATE;

        let mut response = handle.request(req)?;
        while let Some(message) = response.next().await {
            try_nl!(message);
        }
        Ok(())
    }
}
