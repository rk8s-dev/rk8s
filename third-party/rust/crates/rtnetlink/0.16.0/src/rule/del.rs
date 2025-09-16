// SPDX-License-Identifier: MIT

use futures::stream::StreamExt;
use netlink_packet_core::{NetlinkMessage, NLM_F_ACK, NLM_F_REQUEST};
use netlink_packet_route::{rule::RuleMessage, RouteNetlinkMessage};

use crate::{try_nl, Error, Handle};

#[derive(Debug, Clone)]
pub struct RuleDelRequest {
    handle: Handle,
    message: RuleMessage,
}

impl RuleDelRequest {
    pub(crate) fn new(handle: Handle, message: RuleMessage) -> Self {
        RuleDelRequest { handle, message }
    }

    /// Execute the request
    pub async fn execute(self) -> Result<(), Error> {
        let RuleDelRequest {
            mut handle,
            message,
        } = self;

        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::DelRule(message));
        req.header.flags = NLM_F_REQUEST | NLM_F_ACK;
        let mut response = handle.request(req)?;
        while let Some(msg) = response.next().await {
            try_nl!(msg);
        }
        Ok(())
    }

    pub fn message_mut(&mut self) -> &mut RuleMessage {
        &mut self.message
    }
}
