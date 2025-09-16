// SPDX-License-Identifier: MIT

use std::net::Ipv4Addr;
use std::str::FromStr;

use netlink_packet_utils::{Emitable, Parseable};

use crate::{
    route::RouteProtocol,
    rule::{
        flags::RuleFlags, RuleAction, RuleAttribute, RuleHeader, RuleMessage,
        RuleMessageBuffer,
    },
    AddressFamily, IpProtocol,
};

// Setup:
//      ip rule add priority 9000 from 192.0.2.1 to 203.0.113.1 \
//          iif lo oif lo protocol dhcp prohibit
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -4 rule show priority 9000
#[test]
fn test_ipv4_iif_oif_prohibit() {
    let raw = vec![
        0x02, 0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0x10, 0x00, 0x00, 0x00,
        0x07, 0x00, 0x03, 0x00, 0x6c, 0x6f, 0x00, 0x00, 0x07, 0x00, 0x11, 0x00,
        0x6c, 0x6f, 0x00, 0x00, 0x08, 0x00, 0x06, 0x00, 0x28, 0x23, 0x00, 0x00,
        0x08, 0x00, 0x01, 0x00, 0xcb, 0x00, 0x71, 0x01, 0x08, 0x00, 0x02, 0x00,
        0xc0, 0x00, 0x02, 0x01,
    ];

    let expected = RuleMessage {
        header: RuleHeader {
            family: AddressFamily::Inet,
            dst_len: 32,
            src_len: 32,
            tos: 0,
            table: 0,
            action: RuleAction::Prohibit,
            flags: RuleFlags::empty(),
        },
        attributes: vec![
            RuleAttribute::Table(0),
            RuleAttribute::SuppressPrefixLen(0xffffffff),
            RuleAttribute::Protocol(RouteProtocol::Dhcp),
            RuleAttribute::Iifname("lo".to_string()),
            RuleAttribute::Oifname("lo".to_string()),
            RuleAttribute::Priority(9000),
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
//      ip -6 rule add priority 9001 iif lo oif lo ipproto icmp \
//          protocol bgp table 500
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip -6 rule show priority 9000
#[test]
fn test_ipv6_iif_oif_ipproto() {
    let raw = vec![
        0x0a, 0x00, 0x00, 0x00, 0xfc, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x0f, 0x00, 0xf4, 0x01, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00,
        0xff, 0xff, 0xff, 0xff, 0x05, 0x00, 0x15, 0x00, 0xba, 0x00, 0x00, 0x00,
        0x07, 0x00, 0x03, 0x00, 0x6c, 0x6f, 0x00, 0x00, 0x07, 0x00, 0x11, 0x00,
        0x6c, 0x6f, 0x00, 0x00, 0x08, 0x00, 0x06, 0x00, 0x29, 0x23, 0x00, 0x00,
        0x05, 0x00, 0x16, 0x00, 0x01, 0x00, 0x00, 0x00,
    ];

    let expected = RuleMessage {
        header: RuleHeader {
            family: AddressFamily::Inet6,
            dst_len: 0,
            src_len: 0,
            tos: 0,
            table: 252,
            action: RuleAction::ToTable,
            flags: RuleFlags::empty(),
        },
        attributes: vec![
            RuleAttribute::Table(500),
            RuleAttribute::SuppressPrefixLen(0xffffffff),
            RuleAttribute::Protocol(RouteProtocol::Bgp),
            RuleAttribute::Iifname("lo".to_string()),
            RuleAttribute::Oifname("lo".to_string()),
            RuleAttribute::Priority(9001),
            RuleAttribute::IpProtocol(IpProtocol::Icmp),
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
