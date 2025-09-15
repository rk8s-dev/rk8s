// SPDX-License-Identifier: MIT

use std::net::Ipv6Addr;
use std::str::FromStr;

use netlink_packet_utils::traits::{Emitable, Parseable};

use crate::route::flags::RouteFlags;
use crate::route::{
    RouteAttribute, RouteHeader, RouteMessage, RouteMessageBuffer,
    RouteNextHopBuffer, RouteProtocol, RouteScope, RouteType,
};
use crate::AddressFamily;

// wireshark capture(netlink message header removed) of nlmon against command:
//   ip route add 2001:db8:1::/64 dev lo onlink
#[test]
fn test_ipv6_add_route_onlink() {
    let raw = vec![
        0x0a, 0x40, 0x00, 0x00, 0xfe, 0x03, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00,
        0x14, 0x00, 0x01, 0x00, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x04, 0x00,
        0x01, 0x00, 0x00, 0x00,
    ];

    let expected = RouteMessage {
        header: RouteHeader {
            address_family: AddressFamily::Inet6,
            destination_prefix_length: 64,
            source_prefix_length: 0,
            tos: 0,
            table: 254,
            protocol: RouteProtocol::Boot,
            scope: RouteScope::Universe,
            kind: RouteType::Unicast,
            flags: RouteFlags::Onlink,
        },
        attributes: vec![
            RouteAttribute::Destination(
                Ipv6Addr::from_str("2001:db8:1::").unwrap().into(),
            ),
            RouteAttribute::Oif(1),
        ],
    };

    assert_eq!(
        expected,
        RouteMessage::parse(&RouteMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}

// Verify that [`RouteNextHopBuffer`] rejects the buffer when provided with
// an invalid length.
#[test]
fn test_next_hop_max_buffer_len() {
    // Route next-hop buffer layout:
    // |byte0|byte1|byte2|byte3|byte4|byte5|byte6|byte7|bytes8+|
    // |-----|-----|-----|-----|-----|-----|-----|-----|-------|
    // |  length   |flags|hops |    Interface Index    |Payload|
    let buffer = [0xff, 0xff, 0, 0, 0, 0, 0, 0];
    assert!(RouteNextHopBuffer::new_checked(buffer).is_err());
}
