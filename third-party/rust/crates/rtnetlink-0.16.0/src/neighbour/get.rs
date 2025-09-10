// SPDX-License-Identifier: MIT

use futures::{
    future::{self, Either},
    stream::{StreamExt, TryStream},
    FutureExt,
};
use netlink_packet_core::{
    NetlinkMessage, NetlinkPayload, NLM_F_DUMP, NLM_F_REQUEST,
};
use netlink_packet_route::{
    neighbour::{NeighbourFlags, NeighbourMessage},
    RouteNetlinkMessage,
};

use crate::{Error, Handle, IpVersion};

pub struct NeighbourGetRequest {
    handle: Handle,
    message: NeighbourMessage,
}

impl NeighbourGetRequest {
    pub(crate) fn new(handle: Handle) -> Self {
        let message = NeighbourMessage::default();
        NeighbourGetRequest { handle, message }
    }

    /// List neighbor proxies in the system (equivalent to: `ip neighbor show
    /// proxy`).
    pub fn proxies(mut self) -> Self {
        self.message.header.flags |= NeighbourFlags::Proxy;
        self
    }

    pub fn set_family(mut self, ip_version: IpVersion) -> Self {
        self.message.header.family = ip_version.family();
        self
    }

    /// Execute the request
    pub fn execute(
        self,
    ) -> impl TryStream<Ok = NeighbourMessage, Error = Error> {
        let NeighbourGetRequest {
            mut handle,
            message,
        } = self;

        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::GetNeighbour(message));
        req.header.flags = NLM_F_REQUEST | NLM_F_DUMP;

        match handle.request(req) {
            Ok(response) => Either::Left(response.map(move |msg| {
                let (header, payload) = msg.into_parts();
                match payload {
                    NetlinkPayload::InnerMessage(
                        RouteNetlinkMessage::NewNeighbour(msg),
                    ) => Ok(msg),
                    NetlinkPayload::Error(err) => Err(Error::NetlinkError(err)),
                    _ => Err(Error::UnexpectedMessage(NetlinkMessage::new(
                        header, payload,
                    ))),
                }
            })),
            Err(e) => Either::Right(
                future::err::<NeighbourMessage, Error>(e).into_stream(),
            ),
        }
    }

    /// Return a mutable reference to the request
    pub fn message_mut(&mut self) -> &mut NeighbourMessage {
        &mut self.message
    }
}
