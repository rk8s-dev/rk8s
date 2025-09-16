// SPDX-License-Identifier: MIT

use netlink_packet_core::{ErrorMessage, NetlinkMessage};
use netlink_packet_route::RouteNetlinkMessage;
use thiserror::Error;

#[derive(Clone, Eq, PartialEq, Debug, Error)]
pub enum Error {
    #[error("Received an unexpected message {0:?}")]
    UnexpectedMessage(NetlinkMessage<RouteNetlinkMessage>),

    #[error("Received a netlink error message {0}")]
    NetlinkError(ErrorMessage),

    #[error("A netlink request failed")]
    RequestFailed,

    #[error("Namespace error {0}")]
    NamespaceError(String),

    #[error(
        "Received a link message (RTM_GETLINK, RTM_NEWLINK, RTM_SETLINK or RTMGETLINK) with an invalid hardware address attribute: {0:?}."
    )]
    InvalidHardwareAddress(Vec<u8>),

    #[error("Failed to parse an IP address: {0:?}")]
    InvalidIp(Vec<u8>),

    #[error("Failed to parse a network address (IP and mask): {0:?}/{1:?}")]
    InvalidAddress(Vec<u8>, Vec<u8>),

    #[error("Attempting to set and Invalid NLA: {0}")]
    InvalidNla(String),
}
