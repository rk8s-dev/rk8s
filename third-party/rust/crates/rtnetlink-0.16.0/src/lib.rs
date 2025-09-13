// SPDX-License-Identifier: MIT

//! This crate provides methods to manipulate networking resources (links,
//! addresses, arp tables, route tables) via the netlink protocol.

#![allow(clippy::module_inception)]

pub use netlink_packet_core as packet_core;
pub use netlink_packet_route as packet_route;
pub use netlink_packet_utils as packet_utils;
pub use netlink_proto as proto;
pub use netlink_sys as sys;

mod addr;
mod connection;
pub mod constants;
mod errors;
mod handle;
mod link;
mod macros;
mod neighbour;
#[cfg(not(target_os = "freebsd"))]
mod ns;
mod route;
mod rule;
#[cfg(not(target_os = "freebsd"))]
mod traffic_control;

pub use crate::addr::{
    AddressAddRequest, AddressDelRequest, AddressGetRequest, AddressHandle,
};
#[cfg(feature = "tokio_socket")]
pub use crate::connection::new_connection;
pub use crate::connection::new_connection_with_socket;
pub use crate::errors::Error;
pub use crate::handle::Handle;
pub use crate::link::{
    LinkAddRequest, LinkBond, LinkBondPort, LinkBridge, LinkDelPropRequest,
    LinkDelRequest, LinkDummy, LinkGetRequest, LinkHandle, LinkMacVlan,
    LinkMacVtap, LinkMessageBuilder, LinkSetRequest, LinkUnspec, LinkVeth,
    LinkVlan, LinkVrf, LinkVxlan, LinkWireguard, LinkXfrm, QosMapping,
};
pub use crate::neighbour::{
    NeighbourAddRequest, NeighbourDelRequest, NeighbourGetRequest,
    NeighbourHandle,
};
#[cfg(not(target_os = "freebsd"))]
pub use crate::ns::{NetworkNamespace, NETNS_PATH, NONE_FS, SELF_NS_PATH};
pub use crate::route::{
    IpVersion, RouteAddRequest, RouteDelRequest, RouteGetRequest, RouteHandle,
    RouteMessageBuilder,
};
pub use crate::rule::{
    RuleAddRequest, RuleDelRequest, RuleGetRequest, RuleHandle,
};
#[cfg(not(target_os = "freebsd"))]
pub use crate::traffic_control::{
    QDiscDelRequest, QDiscGetRequest, QDiscHandle, QDiscNewRequest,
    TrafficChainGetRequest, TrafficChainHandle, TrafficClassGetRequest,
    TrafficClassHandle, TrafficFilterGetRequest, TrafficFilterHandle,
    TrafficFilterNewRequest,
};
