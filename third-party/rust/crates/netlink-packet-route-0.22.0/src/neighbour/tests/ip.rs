// SPDX-License-Identifier: MIT

use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use netlink_packet_utils::{Emitable, Parseable};

use crate::neighbour::flags::NeighbourFlags;
use crate::{
    neighbour::{
        NeighbourAttribute, NeighbourCacheInfo, NeighbourHeader,
        NeighbourMessage, NeighbourMessageBuffer, NeighbourState,
    },
    route::{RouteProtocol, RouteType},
    AddressFamily,
};

// wireshark capture(netlink message header removed) of nlmon against command:
//   ip -4 neighbour show
#[test]
fn test_ipv4_neighbour_show() {
    let raw = vec![
        0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x01,
        0x08, 0x00, 0x01, 0x00, 0xac, 0x11, 0x02, 0x01, 0x0a, 0x00, 0x02, 0x00,
        0x1c, 0x69, 0x7a, 0x07, 0xc3, 0x36, 0x00, 0x00, 0x08, 0x00, 0x04, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x03, 0x00, 0x71, 0x01, 0x00, 0x00,
        0xcb, 0x0d, 0x1f, 0x00, 0xcb, 0x0d, 0x1f, 0x00, 0x01, 0x00, 0x00, 0x00,
    ];

    let expected = NeighbourMessage {
        header: NeighbourHeader {
            family: AddressFamily::Inet,
            ifindex: 3,
            state: NeighbourState::Reachable,
            flags: NeighbourFlags::empty(),
            kind: RouteType::Unicast,
        },
        attributes: vec![
            NeighbourAttribute::Destination(
                Ipv4Addr::from_str("172.17.2.1").unwrap().into(),
            ),
            NeighbourAttribute::LinkLocalAddress(vec![
                28, 105, 122, 7, 195, 54,
            ]),
            NeighbourAttribute::Probes(1),
            NeighbourAttribute::CacheInfo(NeighbourCacheInfo {
                confirmed: 369,
                used: 2035147,
                updated: 2035147,
                refcnt: 1,
            }),
        ],
    };

    assert_eq!(
        expected,
        NeighbourMessage::parse(&NeighbourMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}

// wireshark capture(netlink message header removed) of nlmon against command:
//   ip -6 neighbour show
#[test]
fn test_ipv6_neighbour_show() {
    let raw = vec![
        0x0a, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x80, 0x01,
        0x14, 0x00, 0x01, 0x00, 0xfe, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x1e, 0x69, 0x7a, 0xff, 0xfe, 0x07, 0xc3, 0x36, 0x0a, 0x00, 0x02, 0x00,
        0x1c, 0x69, 0x7a, 0x07, 0xc3, 0x36, 0x00, 0x00, 0x08, 0x00, 0x04, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x03, 0x00, 0x61, 0x76, 0x00, 0x00,
        0x61, 0x76, 0x00, 0x00, 0x8c, 0x65, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let expected = NeighbourMessage {
        header: NeighbourHeader {
            family: AddressFamily::Inet6,
            ifindex: 3,
            state: NeighbourState::Stale,
            flags: NeighbourFlags::Router,
            kind: RouteType::Unicast,
        },
        attributes: vec![
            NeighbourAttribute::Destination(
                Ipv6Addr::from_str("fe80::1e69:7aff:fe07:c336")
                    .unwrap()
                    .into(),
            ),
            NeighbourAttribute::LinkLocalAddress(vec![
                28, 105, 122, 7, 195, 54,
            ]),
            NeighbourAttribute::Probes(1),
            NeighbourAttribute::CacheInfo(NeighbourCacheInfo {
                confirmed: 30305,
                used: 30305,
                updated: 25996,
                refcnt: 0,
            }),
        ],
    };

    assert_eq!(
        expected,
        NeighbourMessage::parse(&NeighbourMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}

// Setup
//  ip neighbo add 172.17.2.99 dev wlan0 lladdr 00:11:22:33:44:55 \
//      protocol dhcp
// wireshark capture(netlink message header removed) of nlmon against command:
//   ip -4 neighbour show
#[test]
fn test_ipv4_neighbour_protocol_show() {
    let raw = vec![
        0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x01,
        0x08, 0x00, 0x01, 0x00, 0xac, 0x11, 0x02, 0x63, 0x0a, 0x00, 0x02, 0x00,
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x00, 0x00, 0x08, 0x00, 0x04, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x03, 0x00, 0x4d, 0x0b, 0x00, 0x00,
        0x4d, 0x0b, 0x00, 0x00, 0x4d, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x05, 0x00, 0x0c, 0x00, 0x10, 0x00, 0x00, 0x00,
    ];

    let expected = NeighbourMessage {
        header: NeighbourHeader {
            family: AddressFamily::Inet,
            ifindex: 3,
            state: NeighbourState::Permanent,
            flags: NeighbourFlags::empty(),
            kind: RouteType::Unicast,
        },
        attributes: vec![
            NeighbourAttribute::Destination(
                Ipv4Addr::from_str("172.17.2.99").unwrap().into(),
            ),
            NeighbourAttribute::LinkLocalAddress(vec![
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55,
            ]),
            NeighbourAttribute::Probes(0),
            NeighbourAttribute::CacheInfo(NeighbourCacheInfo {
                confirmed: 2893,
                used: 2893,
                updated: 2893,
                refcnt: 0,
            }),
            NeighbourAttribute::Protocol(RouteProtocol::Dhcp),
        ],
    };

    assert_eq!(
        expected,
        NeighbourMessage::parse(&NeighbourMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}
