// SPDX-License-Identifier: MIT

use futures::stream::StreamExt;
use std::{marker::PhantomData, net::IpAddr};

use netlink_packet_core::{
    NetlinkMessage, NLM_F_ACK, NLM_F_CREATE, NLM_F_EXCL, NLM_F_REPLACE,
    NLM_F_REQUEST,
};
use netlink_packet_route::{route::RouteMessage, RouteNetlinkMessage};

use crate::{try_nl, Error, Handle};

/// A request to create a new route. This is equivalent to the `ip route add`
/// commands.
#[derive(Debug, Clone)]
pub struct RouteAddRequest<T = IpAddr> {
    handle: Handle,
    message: RouteMessage,
    replace: bool,
    _phantom: PhantomData<T>,
}

impl<T> RouteAddRequest<T> {
    pub(crate) fn new(handle: Handle, message: RouteMessage) -> Self {
        RouteAddRequest {
            handle,
            message,
            replace: false,
            _phantom: Default::default(),
        }
    }

    pub fn message_mut(&mut self) -> &mut RouteMessage {
        &mut self.message
    }

    /// Replace existing matching route.
    pub fn replace(self) -> Self {
        Self {
            replace: true,
            ..self
        }
    }

    /// Execute the request.
    pub async fn execute(self) -> Result<(), Error> {
        let RouteAddRequest {
            mut handle,
            message,
            replace,
            ..
        } = self;
        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::NewRoute(message));
        let replace = if replace { NLM_F_REPLACE } else { NLM_F_EXCL };
        req.header.flags = NLM_F_REQUEST | NLM_F_ACK | replace | NLM_F_CREATE;

        let mut response = handle.request(req)?;
        while let Some(message) = response.next().await {
            try_nl!(message);
        }
        Ok(())
    }
}
