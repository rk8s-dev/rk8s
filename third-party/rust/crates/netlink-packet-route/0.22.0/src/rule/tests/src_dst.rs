// SPDX-License-Identifier: MIT

use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use netlink_packet_utils::{Emitable, Parseable};

use crate::rule::flags::RuleFlags;
use crate::{
    route::RouteProtocol,
    rule::{
        RuleAction, RuleAttribute, RuleHeader, RuleMessage, RuleMessageBuffer,
    },
    AddressFamily,
};

// Setup:
//      ip rule add priority 1000 from 192.0.2.1 to 203.0.113.1 blackhole
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -4 rule show priority 1000
#[test]
fn test_ipv4_src_dst_blackhole() {
    let raw = vec![
        0x02, 0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x06, 0x00, 0xe8, 0x03, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00,
        0xcb, 0x00, 0x71, 0x01, 0x08, 0x00, 0x02, 0x00, 0xc0, 0x00, 0x02, 0x01,
    ];

    let expected = RuleMessage {
        header: RuleHeader {
            family: AddressFamily::Inet,
            dst_len: 32,
            src_len: 32,
            tos: 0,
            table: 0,
            action: RuleAction::Blackhole,
            flags: RuleFlags::empty(),
        },
        attributes: vec![
            RuleAttribute::Table(0),
            RuleAttribute::SuppressPrefixLen(0xffffffff),
            RuleAttribute::Protocol(RouteProtocol::Unspec),
            RuleAttribute::Priority(1000),
            RuleAttribute::Destination(
                Ipv4Addr::from_str("203.0.113.1").unwrap().into(),
            ),
            RuleAttribute::Source(
                Ipv4Addr::from_str("192.0.2.1").unwrap().into(),
            ),
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
//      ip -6 rule add priority 20000 from 2001:db8:1::1 to 2001:db8:2::1 \
//          goto 32766
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -6 rule show priority 20000
#[test]
fn test_ipv6_src_dst_goto() {
    let raw = vec![
        0x0a, 0x80, 0x80, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x06, 0x00, 0x20, 0x4e, 0x00, 0x00, 0x08, 0x00, 0x04, 0x00,
        0xfe, 0x7f, 0x00, 0x00, 0x14, 0x00, 0x01, 0x00, 0x20, 0x01, 0x0d, 0xb8,
        0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        0x14, 0x00, 0x02, 0x00, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    ];

    let expected = RuleMessage {
        header: RuleHeader {
            family: AddressFamily::Inet6,
            dst_len: 128,
            src_len: 128,
            tos: 0,
            table: 0,
            action: RuleAction::Goto,
            flags: RuleFlags::empty(),
        },
        attributes: vec![
            RuleAttribute::Table(0),
            RuleAttribute::SuppressPrefixLen(0xffffffff),
            RuleAttribute::Protocol(RouteProtocol::Unspec),
            RuleAttribute::Priority(20000),
            RuleAttribute::Goto(32766),
            RuleAttribute::Destination(
                Ipv6Addr::from_str("2001:db8:2::1").unwrap().into(),
            ),
            RuleAttribute::Source(
                Ipv6Addr::from_str("2001:db8:1::1").unwrap().into(),
            ),
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
