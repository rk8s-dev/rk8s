// SPDX-License-Identifier: MIT

use futures::stream::StreamExt;
use netlink_packet_core::{
    NetlinkMessage, NetlinkPayload, NLM_F_ACK, NLM_F_REQUEST,
};
use netlink_packet_route::{route::RouteMessage, RouteNetlinkMessage};

use crate::{Error, Handle};

#[derive(Debug, Clone)]
pub struct RouteDelRequest {
    handle: Handle,
    message: RouteMessage,
}

impl RouteDelRequest {
    pub(crate) fn new(handle: Handle, message: RouteMessage) -> Self {
        RouteDelRequest { handle, message }
    }

    /// Execute the request
    pub async fn execute(self) -> Result<(), Error> {
        let RouteDelRequest {
            mut handle,
            message,
        } = self;

        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::DelRoute(message));
        req.header.flags = NLM_F_REQUEST | NLM_F_ACK;
        let mut response = handle.request(req)?;
        while let Some(msg) = response.next().await {
            if let NetlinkPayload::Error(e) = msg.payload {
                return Err(Error::NetlinkError(e));
            }
        }
        Ok(())
    }

    pub fn message_mut(&mut self) -> &mut RouteMessage {
        &mut self.message
    }
}
