// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::{
    route::{RouteProtocol, RouteRealm},
    rule::{
        flags::RuleFlags, RuleAction, RuleAttribute, RuleHeader, RuleMessage,
        RuleMessageBuffer, RulePortRange,
    },
    AddressFamily, IpProtocol,
};

// Setup:
//      ip rule add priority 1009 sport 80 dport 8080 ipproto tcp realms 199
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -4 rule show priority 1009
#[test]
fn test_ipv4_tcp_sport_dport_realm() {
    let raw = vec![
        0x02, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x06, 0x00, 0xf1, 0x03, 0x00, 0x00, 0x08, 0x00, 0x17, 0x00,
        0x50, 0x00, 0x50, 0x00, 0x08, 0x00, 0x18, 0x00, 0x90, 0x1f, 0x90, 0x1f,
        0x05, 0x00, 0x16, 0x00, 0x06, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0b, 0x00,
        0xc7, 0x00, 0x00, 0x00,
    ];

    let expected = RuleMessage {
        header: RuleHeader {
            family: AddressFamily::Inet,
            dst_len: 0,
            src_len: 0,
            tos: 0,
            table: 254,
            action: RuleAction::ToTable,
            flags: RuleFlags::empty(),
        },
        attributes: vec![
            RuleAttribute::Table(254),
            RuleAttribute::SuppressPrefixLen(0xffffffff),
            RuleAttribute::Protocol(RouteProtocol::Unspec),
            RuleAttribute::Priority(1009),
            RuleAttribute::SourcePortRange(RulePortRange {
                start: 80,
                end: 80,
            }),
            RuleAttribute::DestinationPortRange(RulePortRange {
                start: 8080,
                end: 8080,
            }),
            RuleAttribute::IpProtocol(IpProtocol::Tcp),
            RuleAttribute::Realm(RouteRealm {
                source: 0,
                destination: 199,
            }),
        ],
    };
    assert_eq!(
        expected,
        RuleMessage::parse(&RuleMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}

// Setup:
//      ip -4 rule add priority 1020 sport 80-8080 dport 8080-9090 \
//          ipproto udp realms 199/200
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -4 rule show priority 1020
#[test]
fn test_ipv4_udp_sport_range_dport_range_reals_src_dst() {
    let raw = vec![
        0x02, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x06, 0x00, 0xfc, 0x03, 0x00, 0x00, 0x08, 0x00, 0x17, 0x00,
        0x50, 0x00, 0x90, 0x1f, 0x08, 0x00, 0x18, 0x00, 0x90, 0x1f, 0x82, 0x23,
        0x05, 0x00, 0x16, 0x00, 0x11, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0b, 0x00,
        0xc8, 0x00, 0xc7, 0x00,
    ];

    let expected = RuleMessage {
        header: RuleHeader {
            family: AddressFamily::Inet,
            dst_len: 0,
            src_len: 0,
            tos: 0,
            table: 254,
            action: RuleAction::ToTable,
            flags: RuleFlags::empty(),
        },
        attributes: vec![
            RuleAttribute::Table(254),
            RuleAttribute::SuppressPrefixLen(0xffffffff),
            RuleAttribute::Protocol(RouteProtocol::Unspec),
            RuleAttribute::Priority(1020),
            RuleAttribute::SourcePortRange(RulePortRange {
                start: 80,
                end: 8080,
            }),
            RuleAttribute::DestinationPortRange(RulePortRange {
                start: 8080,
                end: 9090,
            }),
            RuleAttribute::IpProtocol(IpProtocol::Udp),
            RuleAttribute::Realm(RouteRealm {
                source: 199,
                destination: 200,
            }),
        ],
    };
    assert_eq!(
        expected,
        RuleMessage::parse(&RuleMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}
