// SPDX-License-Identifier: MIT

use std::net::Ipv4Addr;
use std::str::FromStr;

use netlink_packet_utils::traits::{Emitable, Parseable};

use crate::route::flags::RouteFlags;
use crate::route::{
    RouteAttribute, RouteCacheInfo, RouteHeader, RouteMessage,
    RouteMessageBuffer, RouteProtocol, RouteRealm, RouteScope, RouteType,
};
use crate::AddressFamily;

// Setup
//   ip route add 192.0.2.1 dev lo realm 250/254
// wireshark capture(netlink message header removed) of nlmon against command:
//   ip route get 192.0.2.1
#[test]
fn test_ipv4_route_realm() {
    let raw = vec![
        0x02, 0x20, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x80,
        0x08, 0x00, 0x0f, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00,
        0xc0, 0x00, 0x02, 0x01, 0x08, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0b, 0x00, 0xfe, 0x00, 0xfa, 0x00, 0x08, 0x00, 0x07, 0x00,
        0xac, 0x11, 0x02, 0x0c, 0x08, 0x00, 0x19, 0x00, 0xe8, 0x03, 0x00, 0x00,
        0x24, 0x00, 0x0c, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let expected = RouteMessage {
        header: RouteHeader {
            address_family: AddressFamily::Inet,
            destination_prefix_length: 32,
            source_prefix_length: 0,
            tos: 0,
            table: 254,
            protocol: RouteProtocol::Unspec,
            scope: RouteScope::Universe,
            kind: RouteType::Unicast,
            flags: RouteFlags::Cloned
                | RouteFlags::from_bits_retain(0x80000000),
        },
        attributes: vec![
            RouteAttribute::Table(254),
            RouteAttribute::Destination(
                Ipv4Addr::from_str("192.0.2.1").unwrap().into(),
            ),
            RouteAttribute::Oif(1),
            RouteAttribute::Realm(RouteRealm {
                source: 250,
                destination: 254,
            }),
            RouteAttribute::PrefSource(
                Ipv4Addr::from_str("172.17.2.12").unwrap().into(),
            ),
            RouteAttribute::Uid(1000),
            RouteAttribute::CacheInfo(RouteCacheInfo {
                clntref: 2,
                last_use: 0,
                expires: 0,
                error: 0,
                used: 0,
                id: 0,
                ts: 0,
                ts_age: 0,
            }),
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
