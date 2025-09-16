// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::{
    route::RouteProtocol,
    rule::{
        flags::RuleFlags, RuleAction, RuleAttribute, RuleHeader, RuleMessage,
        RuleMessageBuffer, RuleUidRange,
    },
    AddressFamily,
};

// Setup:
//      ip rule add l3mdev priority 1999
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -4 rule show priority 1999
#[test]
fn test_ipv4_l3mdev() {
    let raw = vec![
        0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x06, 0x00, 0xcf, 0x07, 0x00, 0x00, 0x05, 0x00, 0x13, 0x00,
        0x01, 0x00, 0x00, 0x00,
    ];

    let expected = RuleMessage {
        header: RuleHeader {
            family: AddressFamily::Inet,
            dst_len: 0,
            src_len: 0,
            tos: 0,
            table: 0,
            action: RuleAction::ToTable,
            flags: RuleFlags::empty(),
        },
        attributes: vec![
            RuleAttribute::Table(0),
            RuleAttribute::SuppressPrefixLen(0xffffffff),
            RuleAttribute::Protocol(RouteProtocol::Unspec),
            RuleAttribute::Priority(1999),
            RuleAttribute::L3MDev(true),
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
//      ip -6 rule add priority 2999 l3mdev uidrange 1000-1999
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -6 rule show priority 2999
#[test]
fn test_ipv6_l3mdev_uid() {
    let raw = vec![
        0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x06, 0x00, 0xb7, 0x0b, 0x00, 0x00, 0x05, 0x00, 0x13, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x14, 0x00, 0xe8, 0x03, 0x00, 0x00,
        0xcf, 0x07, 0x00, 0x00,
    ];

    let expected = RuleMessage {
        header: RuleHeader {
            family: AddressFamily::Inet6,
            dst_len: 0,
            src_len: 0,
            tos: 0,
            table: 0,
            action: RuleAction::ToTable,
            flags: RuleFlags::empty(),
        },
        attributes: vec![
            RuleAttribute::Table(0),
            RuleAttribute::SuppressPrefixLen(0xffffffff),
            RuleAttribute::Protocol(RouteProtocol::Unspec),
            RuleAttribute::Priority(2999),
            RuleAttribute::L3MDev(true),
            RuleAttribute::UidRange(RuleUidRange {
                start: 1000,
                end: 1999,
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
