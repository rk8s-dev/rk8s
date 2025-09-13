// SPDX-License-Identifier: MIT

use futures::{
    future::{self, Either},
    stream::{StreamExt, TryStream, TryStreamExt},
    FutureExt,
};
use std::net::IpAddr;

use netlink_packet_core::{NetlinkMessage, NLM_F_DUMP, NLM_F_REQUEST};
use netlink_packet_route::{
    address::{AddressAttribute, AddressMessage},
    RouteNetlinkMessage,
};

use crate::{try_rtnl, Error, Handle};

pub struct AddressGetRequest {
    handle: Handle,
    message: AddressMessage,
    filter_builder: AddressFilterBuilder,
}

impl AddressGetRequest {
    pub(crate) fn new(handle: Handle) -> Self {
        AddressGetRequest {
            handle,
            message: AddressMessage::default(),
            filter_builder: AddressFilterBuilder::new(),
        }
    }

    pub fn message_mut(&mut self) -> &mut AddressMessage {
        &mut self.message
    }

    pub fn execute(self) -> impl TryStream<Ok = AddressMessage, Error = Error> {
        let AddressGetRequest {
            mut handle,
            message,
            filter_builder,
        } = self;

        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::GetAddress(message));
        req.header.flags = NLM_F_REQUEST | NLM_F_DUMP;

        let filter = filter_builder.build();
        match handle.request(req) {
            Ok(response) => Either::Left(
                response
                    .map(move |msg| {
                        Ok(try_rtnl!(msg, RouteNetlinkMessage::NewAddress))
                    })
                    .try_filter(move |msg| future::ready(filter(msg))),
            ),
            Err(e) => Either::Right(
                future::err::<AddressMessage, Error>(e).into_stream(),
            ),
        }
    }

    /// Return only the addresses of the given interface.
    pub fn set_link_index_filter(mut self, index: u32) -> Self {
        self.filter_builder.index = Some(index);
        self
    }

    /// Return only the addresses of the given prefix length.
    pub fn set_prefix_length_filter(mut self, prefix: u8) -> Self {
        self.filter_builder.prefix_len = Some(prefix);
        self
    }

    /// Return only AddressMessages filtered by the given address.
    pub fn set_address_filter(mut self, address: IpAddr) -> Self {
        self.filter_builder.address = Some(address);
        self
    }
}

// The reason for having filters, is that we cannot retrieve addresses
// that match the given message, like we do for links.
//
// See:
// https://lists.infradead.org/pipermail/libnl/2013-June/001014.html
// https://patchwork.ozlabs.org/patch/133440/
#[derive(Default)]
struct AddressFilterBuilder {
    index: Option<u32>,
    address: Option<IpAddr>,
    prefix_len: Option<u8>,
}

impl AddressFilterBuilder {
    fn new() -> Self {
        Default::default()
    }

    fn build(self) -> impl Fn(&AddressMessage) -> bool {
        use AddressAttribute::*;

        move |msg: &AddressMessage| {
            if let Some(index) = self.index {
                if msg.header.index != index {
                    return false;
                }
            }

            if let Some(prefix_len) = self.prefix_len {
                if msg.header.prefix_len != prefix_len {
                    return false;
                }
            }

            if let Some(address) = self.address {
                for nla in msg.attributes.iter() {
                    if let Address(x) | Local(x) = nla {
                        if x == &address {
                            return true;
                        }
                    } else if let Multicast(x) | Anycast(x) = nla {
                        if IpAddr::V6(*x) == address {
                            return true;
                        }
                    }
                }
                return false;
            }
            true
        }
    }
}
