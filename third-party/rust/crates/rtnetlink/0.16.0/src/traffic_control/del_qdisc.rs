// SPDX-License-Identifier: MIT

use futures::StreamExt;
use netlink_packet_core::{NetlinkMessage, NLM_F_ACK, NLM_F_REQUEST};
use netlink_packet_route::{tc::TcMessage, RouteNetlinkMessage};

use crate::{try_nl, Error, Handle};

#[derive(Debug, Clone)]
pub struct QDiscDelRequest {
    handle: Handle,
    message: TcMessage,
}

impl QDiscDelRequest {
    pub(crate) fn new(handle: Handle, message: TcMessage) -> Self {
        QDiscDelRequest { handle, message }
    }

    // Execute the request
    pub async fn execute(self) -> Result<(), Error> {
        let QDiscDelRequest {
            mut handle,
            message,
        } = self;

        let mut req = NetlinkMessage::from(
            RouteNetlinkMessage::DelQueueDiscipline(message),
        );
        req.header.flags = NLM_F_REQUEST | NLM_F_ACK;

        let mut response = handle.request(req)?;
        while let Some(message) = response.next().await {
            try_nl!(message)
        }
        Ok(())
    }

    /// Return a mutable reference to the request
    pub fn message_mut(&mut self) -> &mut TcMessage {
        &mut self.message
    }
}
