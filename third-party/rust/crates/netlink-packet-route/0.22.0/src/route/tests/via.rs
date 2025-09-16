// SPDX-License-Identifier: MIT

use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use netlink_packet_utils::traits::{Emitable, Parseable};

use crate::route::flags::RouteFlags;
use crate::route::{
    RouteAttribute, RouteHeader, RouteMessage, RouteMessageBuffer,
    RouteProtocol, RouteScope, RouteType, RouteVia,
};
use crate::AddressFamily;

// Setup:
//      ip route add 192.0.2.1 via inet6 2001:db8:1:: dev lo
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip route show dev lo
#[test]
fn test_ipv4_route_via() {
    let raw = vec![
        0x02, 0x20, 0x00, 0x00, 0xfe, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00,
        0xc0, 0x00, 0x02, 0x01, 0x16, 0x00, 0x12, 0x00, 0x0a, 0x00, 0x20, 0x01,
        0x0d, 0xb8, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00,
    ];

    let expected = RouteMessage {
        header: RouteHeader {
            address_family: AddressFamily::Inet,
            destination_prefix_length: 32,
            source_prefix_length: 0,
            tos: 0,
            table: 254,
            protocol: RouteProtocol::Boot,
            scope: RouteScope::Universe,
            kind: RouteType::Unicast,
            flags: RouteFlags::empty(),
        },
        attributes: vec![
            RouteAttribute::Table(254),
            RouteAttribute::Destination(
                Ipv4Addr::from_str("192.0.2.1").unwrap().into(),
            ),
            RouteAttribute::Via(RouteVia::Inet6(
                Ipv6Addr::from_str("2001:db8:1::").unwrap(),
            )),
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
