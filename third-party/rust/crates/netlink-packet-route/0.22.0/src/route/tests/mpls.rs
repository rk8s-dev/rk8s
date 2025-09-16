// SPDX-License-Identifier: MIT

use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use netlink_packet_utils::traits::{Emitable, Parseable};

use crate::route::flags::RouteFlags;
use crate::route::{
    MplsLabel, RouteAddress, RouteAttribute, RouteCacheInfo, RouteHeader,
    RouteLwEnCapType, RouteLwTunnelEncap, RouteMessage, RouteMessageBuffer,
    RouteMplsIpTunnel, RouteMplsTtlPropagation, RoutePreference, RouteProtocol,
    RouteScope, RouteType,
};
use crate::AddressFamily;

// Setup:
//      ip link add dummy1 type dummy
//      ip link set dummy1 up
//      ip addr add 192.0.2.1/24 dev dummy1
//      modprobe mpls_iptunnel
//      sysctl -w net.mpls.platform_labels=65535
//      ip route add 198.51.100.1/32 encap mpls 100 ttl 25 \
//          via inet 192.0.2.1 dev dummy1
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip route show dev dummy1
#[test]
fn test_mpls_route_to_ipv4() {
    let raw = vec![
        0x02, 0x20, 0x00, 0x00, 0xfe, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00,
        0xc6, 0x33, 0x64, 0x01, 0x08, 0x00, 0x05, 0x00, 0xc0, 0x00, 0x02, 0x01,
        0x08, 0x00, 0x04, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x14, 0x00, 0x16, 0x00,
        0x08, 0x00, 0x01, 0x00, 0x00, 0x06, 0x41, 0x00, 0x05, 0x00, 0x02, 0x00,
        0x19, 0x00, 0x00, 0x00, 0x06, 0x00, 0x15, 0x00, 0x01, 0x00, 0x00, 0x00,
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
                Ipv4Addr::from_str("198.51.100.1").unwrap().into(),
            ),
            RouteAttribute::Gateway(
                Ipv4Addr::from_str("192.0.2.1").unwrap().into(),
            ),
            RouteAttribute::Oif(10),
            RouteAttribute::Encap(vec![
                RouteLwTunnelEncap::Mpls(RouteMplsIpTunnel::Destination(vec![
                    MplsLabel {
                        label: 100,
                        traffic_class: 0,
                        bottom_of_stack: true,
                        ttl: 0,
                    },
                ])),
                RouteLwTunnelEncap::Mpls(RouteMplsIpTunnel::Ttl(25)),
            ]),
            RouteAttribute::EncapType(RouteLwEnCapType::Mpls),
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

// Setup:
//      ip link add dummy1 type dummy
//      ip link set dummy1 up
//      ip addr add 2001:db8:1::1/64 dev dummy1
//      modprobe mpls_iptunnel
//      sysctl -w net.mpls.platform_labels=65535
//      ip route add 2001:db8:2::/64 encap mpls 200 \
//          via inet6 2001:db8:1::2 dev dummy1
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -6 route show dev dummy1
#[test]
fn test_ipv6_to_mpls_route() {
    let raw = vec![
        0x0a, 0x40, 0x00, 0x00, 0xfe, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x14, 0x00, 0x01, 0x00,
        0x20, 0x01, 0x0d, 0xb8, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x06, 0x00, 0x00, 0x04, 0x00, 0x00,
        0x14, 0x00, 0x05, 0x00, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x08, 0x00, 0x04, 0x00,
        0x07, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x16, 0x00, 0x08, 0x00, 0x01, 0x00,
        0x00, 0x0c, 0x81, 0x00, 0x06, 0x00, 0x15, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x24, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x05, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00,
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
            flags: RouteFlags::empty(),
        },
        attributes: vec![
            RouteAttribute::Table(254),
            RouteAttribute::Destination(
                Ipv6Addr::from_str("2001:db8:2::").unwrap().into(),
            ),
            RouteAttribute::Priority(1024),
            RouteAttribute::Gateway(
                Ipv6Addr::from_str("2001:db8:1::2").unwrap().into(),
            ),
            RouteAttribute::Oif(7),
            RouteAttribute::Encap(vec![RouteLwTunnelEncap::Mpls(
                RouteMplsIpTunnel::Destination(vec![MplsLabel {
                    label: 200,
                    traffic_class: 0,
                    bottom_of_stack: true,
                    ttl: 0,
                }]),
            )]),
            RouteAttribute::EncapType(RouteLwEnCapType::Mpls),
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

// Setup:
//      ip link add dummy1 type dummy
//      ip link set dummy1 up
//      ip addr add 2001:db8:1::1/64 dev dummy1
//      modprobe mpls_iptunnel
//      sysctl -w net.mpls.platform_labels=65535
//      ip -f mpls route add 300 via inet6 2001:db8:1::2 dev dummy1
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -f mpls route show dev dummy1
#[test]
fn test_mpls_route_to_ipv6() {
    let raw = vec![
        0x1c, 0x14, 0x00, 0x00, 0xfe, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x01, 0x00, 0x00, 0x12, 0xc1, 0x00, 0x16, 0x00, 0x12, 0x00,
        0x0a, 0x00, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x08, 0x00, 0x04, 0x00,
        0x07, 0x00, 0x00, 0x00,
    ];

    let expected = RouteMessage {
        header: RouteHeader {
            address_family: AddressFamily::Mpls,
            destination_prefix_length: 20,
            source_prefix_length: 0,
            tos: 0,
            table: 254,
            protocol: RouteProtocol::Boot,
            scope: RouteScope::Universe,
            kind: RouteType::Unicast,
            flags: RouteFlags::empty(),
        },
        attributes: vec![
            RouteAttribute::Destination(RouteAddress::Mpls(MplsLabel {
                label: 300,
                traffic_class: 0,
                bottom_of_stack: true,
                ttl: 0,
            })),
            RouteAttribute::Via(
                Ipv6Addr::from_str("2001:db8:1::2").unwrap().into(),
            ),
            RouteAttribute::Oif(7),
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

// Setup:
//      ip link add dummy1 type dummy
//      ip link set dummy1 up
//      ip addr add 2001:db8:1::1/64 dev dummy1
//      modprobe mpls_iptunnel
//      sysctl -w net.mpls.platform_labels=65535
//      ip -f mpls route add 100 as 200 via inet6 2001:db8:1::2 dev dummy1
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -f mpls route show dev dummy1
#[test]
fn test_mpls_route_relable_new_dst() {
    let raw = vec![
        0x1c, 0x14, 0x00, 0x00, 0xfe, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x01, 0x00, 0x00, 0x06, 0x41, 0x00, 0x08, 0x00, 0x13, 0x00,
        0x00, 0x0c, 0x81, 0x00, 0x16, 0x00, 0x12, 0x00, 0x0a, 0x00, 0x20, 0x01,
        0x0d, 0xb8, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x02, 0x00, 0x00, 0x08, 0x00, 0x04, 0x00, 0x09, 0x00, 0x00, 0x00,
    ];

    let expected = RouteMessage {
        header: RouteHeader {
            address_family: AddressFamily::Mpls,
            destination_prefix_length: 20,
            source_prefix_length: 0,
            tos: 0,
            table: 254,
            protocol: RouteProtocol::Boot,
            scope: RouteScope::Universe,
            kind: RouteType::Unicast,
            flags: RouteFlags::empty(),
        },
        attributes: vec![
            RouteAttribute::Destination(RouteAddress::Mpls(MplsLabel {
                label: 100,
                traffic_class: 0,
                bottom_of_stack: true,
                ttl: 0,
            })),
            RouteAttribute::NewDestination(vec![MplsLabel {
                label: 200,
                traffic_class: 0,
                bottom_of_stack: true,
                ttl: 0,
            }]),
            RouteAttribute::Via(
                Ipv6Addr::from_str("2001:db8:1::2").unwrap().into(),
            ),
            RouteAttribute::Oif(9),
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

// Setup:
//      ip link add dummy1 type dummy
//      ip link set dummy1 up
//      ip addr add 2001:db8:1::1/64 dev dummy1
//      modprobe mpls_iptunnel
//      sysctl -w net.mpls.platform_labels=65535
//      ip -f mpls route add 100 ttl-propagate enabled as 200 \
//          via inet6 2001:db8:1::2 dev dummy1
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -6 route show dev dummy1
#[test]
fn test_mpls_ttl_propagate() {
    let raw = vec![
        0x1c, 0x14, 0x00, 0x00, 0xfe, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x01, 0x00, 0x00, 0x06, 0x41, 0x00, 0x05, 0x00, 0x1a, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x13, 0x00, 0x00, 0x0c, 0x81, 0x00,
        0x16, 0x00, 0x12, 0x00, 0x0a, 0x00, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x01,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
        0x08, 0x00, 0x04, 0x00, 0x08, 0x00, 0x00, 0x00,
    ];

    let expected = RouteMessage {
        header: RouteHeader {
            address_family: AddressFamily::Mpls,
            destination_prefix_length: 20,
            source_prefix_length: 0,
            tos: 0,
            table: 254,
            protocol: RouteProtocol::Boot,
            scope: RouteScope::Universe,
            kind: RouteType::Unicast,
            flags: RouteFlags::empty(),
        },
        attributes: vec![
            RouteAttribute::Destination(RouteAddress::Mpls(MplsLabel {
                label: 100,
                traffic_class: 0,
                bottom_of_stack: true,
                ttl: 0,
            })),
            RouteAttribute::TtlPropagate(RouteMplsTtlPropagation::Enabled),
            RouteAttribute::NewDestination(vec![MplsLabel {
                label: 200,
                traffic_class: 0,
                bottom_of_stack: true,
                ttl: 0,
            }]),
            RouteAttribute::Via(
                Ipv6Addr::from_str("2001:db8:1::2").unwrap().into(),
            ),
            RouteAttribute::Oif(8),
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
