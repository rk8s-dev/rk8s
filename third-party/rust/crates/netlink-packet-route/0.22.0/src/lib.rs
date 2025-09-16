// SPDX-License-Identifier: MIT

//! The `netlink-packet-route` crate is designed to abstract Netlink route
//! protocol(`rtnetlink`) packet into Rust data types. The goal of this crate is
//! saving netlink user from reading Kernel Netlink codes.
//!
//! This crate grouped Netlink route protocol into these modules:
//!  * `link`: NIC interface, similar to to `ip link` command.
//!  * `address`: IP address, similar to `ip address` command.
//!  * `route`: Route, similar to `ip route` command.
//!  * `rule`: Route rule, similar to `ip rule` command.
//!  * `tc`: Traffic control, similar to `tc` command.
//!  * `neighbour`: Neighbour, similar to `ip neighbour` command.
//!  * `neighbour_table`: Neighbour table, similar to `ip ntable` command.
//!  * `nsid`: Namespace, similar to `ip netns` command.
//!
//! At the top level of this crate, we also provide:
//!  * [AddressFamily]
//!
//! Normally, you should use [`rtnetlink`][rtnetlink_url] instead of using this
//! crate directly.
//!
//! [rtnetlink_url]: https://docs.rs/rtnetlink

pub mod address;
pub mod link;
pub mod neighbour;
pub mod neighbour_table;
pub mod nsid;
pub mod prefix;
pub mod route;
pub mod rule;
pub mod tc;

mod message;
#[cfg(test)]
mod tests;

pub(crate) mod ip;

#[cfg(any(target_os = "linux", target_os = "fuchsia"))]
mod address_family_linux;
#[cfg(any(target_os = "linux", target_os = "fuchsia"))]
pub use self::address_family_linux::AddressFamily;

#[cfg(target_os = "freebsd")]
mod address_family_freebsd;
#[cfg(target_os = "freebsd")]
pub use self::address_family_freebsd::AddressFamily;

#[cfg(not(any(
    target_os = "linux",
    target_os = "fuchsia",
    target_os = "freebsd",
)))]
mod address_family_fallback;
#[cfg(not(any(
    target_os = "linux",
    target_os = "fuchsia",
    target_os = "freebsd",
)))]
pub use self::address_family_fallback::AddressFamily;

pub use self::ip::IpProtocol;
pub use self::message::{RouteNetlinkMessage, RouteNetlinkMessageBuffer};

#[macro_use]
extern crate netlink_packet_utils;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

#[macro_use]
extern crate bitflags;
