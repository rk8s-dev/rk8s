// SPDX-License-Identifier: MIT

use futures::stream::StreamExt;
use netlink_packet_core::{NetlinkMessage, NLM_F_ACK, NLM_F_REQUEST};
use netlink_packet_route::{
    tc::{TcHandle, TcMessage},
    RouteNetlinkMessage,
};

use crate::{try_nl, Error, Handle};

#[derive(Debug, Clone)]
pub struct TrafficFilterDelRequest {
    handle: Handle,
    message: TcMessage,
    flags: u16,
}

impl TrafficFilterDelRequest {
    pub(crate) fn new(handle: Handle, ifindex: i32) -> Self {
        Self {
            handle,
            message: TcMessage::with_index(ifindex),
            flags: NLM_F_REQUEST | NLM_F_ACK,
        }
    }

    /// Execute the request
    pub async fn execute(self) -> Result<(), Error> {
        let Self {
            mut handle,
            message,
            flags,
        } = self;

        let mut req = NetlinkMessage::from(
            RouteNetlinkMessage::DelTrafficFilter(message),
        );
        req.header.flags = flags;

        let mut response = handle.request(req)?;
        if let Some(message) = response.next().await {
            try_nl!(message);
        }

        Ok(())
    }

    /// Set parent.
    /// Equivalent to `[ root | ingress | egress | parent CLASSID ]`
    /// command args. They are mutually exclusive.
    pub fn parent(mut self, parent: u32) -> Self {
        self.message.header.parent = parent.into();
        self
    }

    /// Set parent to root.
    pub fn root(mut self) -> Self {
        self.message.header.parent = TcHandle::ROOT;
        self
    }

    /// Set parent to ingress.
    pub fn ingress(mut self) -> Self {
        self.message.header.parent = TcHandle {
            major: 0xffff,
            minor: TcHandle::MIN_INGRESS,
        };
        self
    }

    /// Set parent to egress.
    pub fn egress(mut self) -> Self {
        self.message.header.parent = TcHandle {
            major: 0xffff,
            minor: TcHandle::MIN_EGRESS,
        };
        self
    }
}
