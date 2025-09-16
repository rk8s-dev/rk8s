// SPDX-License-Identifier: MIT

use futures::{
    future::{self, Either},
    stream::{StreamExt, TryStream},
    FutureExt,
};
use netlink_packet_core::{NetlinkMessage, NLM_F_DUMP, NLM_F_REQUEST};
use netlink_packet_route::{
    link::{LinkAttribute, LinkExtentMask, LinkMessage},
    AddressFamily, RouteNetlinkMessage,
};

use crate::{try_rtnl, Error, Handle};

pub struct LinkGetRequest {
    handle: Handle,
    message: LinkMessage,
    // There are two ways to retrieve links: we can either dump them
    // all and filter the result, or if we already know the index or
    // the name of the link we're looking for, we can just retrieve
    // that one. If `dump` is `true`, all the links are fetched.
    // Otherwise, only the link that match the given index or name
    // is fetched.
    dump: bool,
}

impl LinkGetRequest {
    pub(crate) fn new(handle: Handle) -> Self {
        LinkGetRequest {
            handle,
            message: LinkMessage::default(),
            dump: true,
        }
    }

    /// Setting filter mask
    pub fn set_filter_mask(
        mut self,
        family: AddressFamily,
        filter_mask: Vec<LinkExtentMask>,
    ) -> Self {
        self.message.header.interface_family = family;
        self.message
            .attributes
            .push(LinkAttribute::ExtMask(filter_mask));
        self
    }

    /// Execute the request
    pub fn execute(self) -> impl TryStream<Ok = LinkMessage, Error = Error> {
        let LinkGetRequest {
            mut handle,
            message,
            dump,
        } = self;

        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::GetLink(message));

        if dump {
            req.header.flags = NLM_F_REQUEST | NLM_F_DUMP;
        } else {
            req.header.flags = NLM_F_REQUEST;
        }

        match handle.request(req) {
            Ok(response) => Either::Left(response.map(move |msg| {
                Ok(try_rtnl!(msg, RouteNetlinkMessage::NewLink))
            })),
            Err(e) => Either::Right(
                future::err::<LinkMessage, Error>(e).into_stream(),
            ),
        }
    }

    /// Return a mutable reference to the request
    pub fn message_mut(&mut self) -> &mut LinkMessage {
        &mut self.message
    }

    /// Lookup a link by index
    pub fn match_index(mut self, index: u32) -> Self {
        self.dump = false;
        self.message.header.index = index;
        self
    }

    /// Lookup a link by name
    ///
    /// This function requires support from your kernel (>= 2.6.33). If yours is
    /// older, consider filtering the resulting stream of links.
    pub fn match_name(mut self, name: String) -> Self {
        self.dump = false;
        self.message.attributes.push(LinkAttribute::IfName(name));
        self
    }
}
