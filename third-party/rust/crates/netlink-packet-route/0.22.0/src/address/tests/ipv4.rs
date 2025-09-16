// SPDX-License-Identifier: MIT

use std::net::{IpAddr, Ipv4Addr};

use netlink_packet_utils::{Emitable, Parseable};

use crate::address::{
    AddressAttribute, AddressFlags, AddressHeader, AddressHeaderFlags,
    AddressMessage, AddressMessageBuffer, AddressScope, CacheInfo,
};
use crate::AddressFamily;

// TODO(Gris Ge): Need test for `AddressAttribute::Broadcast`

#[test]
fn test_ipv4_get_loopback_address() {
    let raw = vec![
        0x02, 0x08, 0x80, 0xfe, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00,
        0x7f, 0x00, 0x00, 0x01, 0x08, 0x00, 0x02, 0x00, 0x7f, 0x00, 0x00, 0x01,
        0x07, 0x00, 0x03, 0x00, 0x6c, 0x6f, 0x00, 0x00, 0x08, 0x00, 0x08, 0x00,
        0x80, 0x00, 0x00, 0x00, 0x14, 0x00, 0x06, 0x00, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0x9c, 0x00, 0x00, 0x00, 0x9c, 0x00, 0x00, 0x00,
    ];

    let expected = AddressMessage {
        header: AddressHeader {
            family: AddressFamily::Inet,
            prefix_len: 8,
            flags: AddressHeaderFlags::Permanent,
            scope: AddressScope::Host,
            index: 1,
        },
        attributes: vec![
            AddressAttribute::Address(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            AddressAttribute::Local(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            AddressAttribute::Label("lo".to_string()),
            AddressAttribute::Flags(AddressFlags::Permanent),
            AddressAttribute::CacheInfo(CacheInfo {
                ifa_preferred: u32::MAX,
                ifa_valid: u32::MAX,
                cstamp: 156,
                tstamp: 156,
            }),
        ],
    };

    assert_eq!(
        expected,
        AddressMessage::parse(&AddressMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}
