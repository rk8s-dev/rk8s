// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::{
    route::RouteProtocol,
    rule::{
        flags::RuleFlags, RuleAction, RuleAttribute, RuleHeader, RuleMessage,
        RuleMessageBuffer,
    },
    AddressFamily,
};

// Setup:
//      ip rule add priority 1001 fwmark 0x20 suppress_prefixlength 8
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -4 rule show priority 1001
#[test]
fn test_ipv4_fwmark_suppress_prefixlength() {
    let raw = vec![
        0x02, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0x08, 0x00, 0x00, 0x00, 0x05, 0x00, 0x15, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x06, 0x00, 0xe9, 0x03, 0x00, 0x00, 0x08, 0x00, 0x0a, 0x00,
        0x20, 0x00, 0x00, 0x00, 0x08, 0x00, 0x10, 0x00, 0xff, 0xff, 0xff, 0xff,
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
            RuleAttribute::SuppressPrefixLen(8),
            RuleAttribute::Protocol(RouteProtocol::Unspec),
            RuleAttribute::Priority(1001),
            RuleAttribute::FwMark(0x20),
            RuleAttribute::FwMask(0xffffffff),
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
//      ip -6 rule add priority 1002 fwmark 0x20 suppress_ifgroup 89
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -6 rule show priority 1002
#[test]
fn test_ipv6_fwmark_suppress_ifgroup() {
    let raw = vec![
        0x0a, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x06, 0x00, 0xea, 0x03, 0x00, 0x00, 0x08, 0x00, 0x0a, 0x00,
        0x20, 0x00, 0x00, 0x00, 0x08, 0x00, 0x10, 0x00, 0xff, 0xff, 0xff, 0xff,
        0x08, 0x00, 0x0d, 0x00, 0x59, 0x00, 0x00, 0x00,
    ];

    let expected = RuleMessage {
        header: RuleHeader {
            family: AddressFamily::Inet6,
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
            RuleAttribute::Priority(1002),
            RuleAttribute::FwMark(0x20),
            RuleAttribute::FwMask(0xffffffff),
            RuleAttribute::SuppressIfGroup(89),
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
