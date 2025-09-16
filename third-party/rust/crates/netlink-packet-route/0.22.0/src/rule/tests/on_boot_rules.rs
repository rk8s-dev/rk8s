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

// wireshark capture(netlink message header removed) of nlmon against command:
//   ip -4 rule show
#[test]
fn test_ipv4_rule() {
    let raw = vec![
        0x02, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0x02, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x06, 0x00, 0xfe, 0x7f, 0x00, 0x00,
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
            RuleAttribute::Protocol(RouteProtocol::Kernel),
            RuleAttribute::Priority(32766),
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

// wireshark capture(netlink message header removed) of nlmon against command:
//   ip -6 rule show
#[test]
fn test_ipv6_rule() {
    let raw = vec![
        0x0a, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0x02, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x06, 0x00, 0xfe, 0x7f, 0x00, 0x00,
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
            RuleAttribute::Protocol(RouteProtocol::Kernel),
            RuleAttribute::Priority(32766),
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
