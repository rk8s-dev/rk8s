// SPDX-License-Identifier: MIT

use futures::{
    future::{self, Either},
    stream::{StreamExt, TryStream},
    FutureExt,
};
use netlink_packet_core::{NetlinkMessage, NLM_F_DUMP, NLM_F_REQUEST};
use netlink_packet_route::{
    tc::{TcHandle, TcMessage},
    RouteNetlinkMessage,
};

use crate::{try_rtnl, Error, Handle};

#[derive(Debug, Clone)]
pub struct QDiscGetRequest {
    handle: Handle,
    message: TcMessage,
}

impl QDiscGetRequest {
    pub(crate) fn new(handle: Handle) -> Self {
        QDiscGetRequest {
            handle,
            message: TcMessage::default(),
        }
    }

    /// Execute the request
    pub fn execute(self) -> impl TryStream<Ok = TcMessage, Error = Error> {
        let QDiscGetRequest {
            mut handle,
            message,
        } = self;

        let mut req = NetlinkMessage::from(
            RouteNetlinkMessage::GetQueueDiscipline(message),
        );
        req.header.flags = NLM_F_REQUEST | NLM_F_DUMP;

        match handle.request(req) {
            Ok(response) => Either::Left(response.map(move |msg| {
                Ok(try_rtnl!(msg, RouteNetlinkMessage::NewQueueDiscipline))
            })),
            Err(e) => {
                Either::Right(future::err::<TcMessage, Error>(e).into_stream())
            }
        }
    }

    pub fn index(mut self, index: i32) -> Self {
        self.message.header.index = index;
        self
    }

    /// Get ingress qdisc
    pub fn ingress(mut self) -> Self {
        self.message.header.parent = TcHandle::INGRESS;
        self
    }
}

#[derive(Debug, Clone)]
pub struct TrafficClassGetRequest {
    handle: Handle,
    message: TcMessage,
}

impl TrafficClassGetRequest {
    pub(crate) fn new(handle: Handle, ifindex: i32) -> Self {
        let mut message = TcMessage::default();
        message.header.index = ifindex;
        TrafficClassGetRequest { handle, message }
    }

    /// Execute the request
    pub fn execute(self) -> impl TryStream<Ok = TcMessage, Error = Error> {
        let TrafficClassGetRequest {
            mut handle,
            message,
        } = self;

        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::GetTrafficClass(message));
        req.header.flags = NLM_F_REQUEST | NLM_F_DUMP;

        match handle.request(req) {
            Ok(response) => Either::Left(response.map(move |msg| {
                Ok(try_rtnl!(msg, RouteNetlinkMessage::NewTrafficClass))
            })),
            Err(e) => {
                Either::Right(future::err::<TcMessage, Error>(e).into_stream())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrafficFilterGetRequest {
    handle: Handle,
    message: TcMessage,
}

impl TrafficFilterGetRequest {
    pub(crate) fn new(handle: Handle, ifindex: i32) -> Self {
        let mut message = TcMessage::default();
        message.header.index = ifindex;
        TrafficFilterGetRequest { handle, message }
    }

    /// Execute the request
    pub fn execute(self) -> impl TryStream<Ok = TcMessage, Error = Error> {
        let TrafficFilterGetRequest {
            mut handle,
            message,
        } = self;

        let mut req = NetlinkMessage::from(
            RouteNetlinkMessage::GetTrafficFilter(message),
        );
        req.header.flags = NLM_F_REQUEST | NLM_F_DUMP;

        match handle.request(req) {
            Ok(response) => Either::Left(response.map(move |msg| {
                Ok(try_rtnl!(msg, RouteNetlinkMessage::NewTrafficFilter))
            })),
            Err(e) => {
                Either::Right(future::err::<TcMessage, Error>(e).into_stream())
            }
        }
    }

    /// Set parent to root.
    pub fn root(mut self) -> Self {
        self.message.header.parent = TcHandle::ROOT;
        self
    }
}

#[derive(Debug, Clone)]
pub struct TrafficChainGetRequest {
    handle: Handle,
    message: TcMessage,
}

impl TrafficChainGetRequest {
    pub(crate) fn new(handle: Handle, ifindex: i32) -> Self {
        let mut message = TcMessage::default();
        message.header.index = ifindex;
        TrafficChainGetRequest { handle, message }
    }

    /// Execute the request
    pub fn execute(self) -> impl TryStream<Ok = TcMessage, Error = Error> {
        let TrafficChainGetRequest {
            mut handle,
            message,
        } = self;

        let mut req =
            NetlinkMessage::from(RouteNetlinkMessage::GetTrafficChain(message));
        req.header.flags = NLM_F_REQUEST | NLM_F_DUMP;

        match handle.request(req) {
            Ok(response) => Either::Left(response.map(move |msg| {
                Ok(try_rtnl!(msg, RouteNetlinkMessage::NewTrafficChain))
            })),
            Err(e) => {
                Either::Right(future::err::<TcMessage, Error>(e).into_stream())
            }
        }
    }
}
