// SPDX-License-Identifier: MIT

use netlink_packet_utils::traits::{Emitable, Parseable};
use std::net::{Ipv4Addr, Ipv6Addr};

use crate::route::flags::RouteFlags;
use crate::route::{
    RouteAttribute, RouteCacheInfo, RouteHeader, RouteMessage,
    RouteMessageBuffer, RoutePreference, RouteProtocol, RouteScope, RouteType,
};
use crate::AddressFamily;

#[test]
// wireshark capture(netlink message header removed) of nlmon against command:
//   ip -4 route show dev lo table local
fn test_ipv4_route_loopback() {
    let raw = vec![
        0x02, 0x08, 0x00, 0x00, 0xff, 0x02, 0xfe, 0x02, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xff, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00,
        0x7f, 0x00, 0x00, 0x00, 0x08, 0x00, 0x07, 0x00, 0x7f, 0x00, 0x00, 0x01,
        0x08, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00,
    ];

    let expected = RouteMessage {
        header: RouteHeader {
            address_family: AddressFamily::Inet,
            destination_prefix_length: 8,
            source_prefix_length: 0,
            tos: 0,
            table: 255,
            protocol: RouteProtocol::Kernel,
            scope: RouteScope::Host,
            kind: RouteType::Local,
            flags: RouteFlags::empty(),
        },
        attributes: vec![
            RouteAttribute::Table(255),
            RouteAttribute::Destination(
                Ipv4Addr::from([127u8, 0, 0, 0]).into(),
            ),
            RouteAttribute::PrefSource(Ipv4Addr::from([127u8, 0, 0, 1]).into()),
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

#[test]
// wireshark capture(netlink message header removed) of nlmon against command:
//   ip -4 route show dev lo table local
fn test_ipv4_route_loopback_broadcast() {
    let raw = vec![
        0x02, 0x20, 0x00, 0x00, 0xff, 0x02, 0xfd, 0x03, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xff, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00,
        0x7f, 0xff, 0xff, 0xff, 0x08, 0x00, 0x07, 0x00, 0x7f, 0x00, 0x00, 0x01,
        0x08, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00,
    ];

    let expected = RouteMessage {
        header: RouteHeader {
            address_family: AddressFamily::Inet,
            destination_prefix_length: 32,
            source_prefix_length: 0,
            tos: 0,
            table: 255,
            protocol: RouteProtocol::Kernel,
            scope: RouteScope::Link,
            kind: RouteType::Broadcast,
            flags: RouteFlags::empty(),
        },
        attributes: vec![
            RouteAttribute::Table(255),
            RouteAttribute::Destination(
                Ipv4Addr::from([127u8, 255, 255, 255]).into(),
            ),
            RouteAttribute::PrefSource(Ipv4Addr::from([127u8, 0, 0, 1]).into()),
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

#[test]
// wireshark capture(netlink message header removed) of nlmon against command:
//   ip -6 route show dev lo table local
fn test_ipv6_route_loopback() {
    let raw = vec![
        0x0a, 0x80, 0x00, 0x00, 0xff, 0x02, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xff, 0x00, 0x00, 0x00, 0x14, 0x00, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00, 0x24, 0x00, 0x0c, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x14, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    let expected = RouteMessage {
        header: RouteHeader {
            address_family: AddressFamily::Inet6,
            destination_prefix_length: 128,
            source_prefix_length: 0,
            tos: 0,
            table: 255,
            protocol: RouteProtocol::Kernel,
            scope: RouteScope::Universe,
            kind: RouteType::Local,
            flags: RouteFlags::empty(),
        },
        attributes: vec![
            RouteAttribute::Table(255),
            RouteAttribute::Destination(Ipv6Addr::LOCALHOST.into()),
            RouteAttribute::Priority(0),
            RouteAttribute::Oif(1),
            RouteAttribute::CacheInfo(RouteCacheInfo {
                clntref: 0,
                last_use: 0,
                expires: 0,
                error: 0,
                used: 0,
                id: 0,
                ts: 0,
                ts_age: 0,
            }),
            RouteAttribute::Preference(RoutePreference::Medium),
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
