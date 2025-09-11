// SPDX-License-Identifier: MIT

use std::net::{IpAddr, Ipv6Addr};

use netlink_packet_utils::{nla::NlaBuffer, Emitable, Parseable};

use crate::address::{
    AddressAttribute, AddressFlags, AddressHeader, AddressHeaderFlags,
    AddressMessage, AddressMessageBuffer, AddressScope, CacheInfo,
};
use crate::AddressFamily;

// TODO(Gris Ge): Need test for `AddressAttribute::Anycast` and `Multicast`.

#[test]
fn test_addr_flag_stable_privacy() {
    let nla = AddressAttribute::Flags(
        AddressFlags::Permanent | AddressFlags::StablePrivacy,
    );

    let raw: [u8; 8] = [
        0x08, 0x00, // length 8
        0x08, 0x00, // IFA_FLAGS
        0x80, 0x08, 0x00, 0x00, // IFA_F_PERMANENT | IFA_F_STABLE_PRIVACY
    ];

    let nla_buffer = NlaBuffer::new_checked(&raw).unwrap();
    let parsed = AddressAttribute::parse(&nla_buffer).unwrap();
    assert_eq!(parsed, nla);

    assert_eq!(nla.buffer_len(), 8);

    let mut buffer: [u8; 8] = [0; 8];
    nla.emit(&mut buffer);
    assert_eq!(buffer, raw);
}

#[test]
fn test_get_loopback_ipv6_addr() {
    let raw = vec![
        0x0a, 0x80, 0x80, 0xfe, 0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x14, 0x00, 0x06, 0x00, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0x8e, 0x00, 0x00, 0x00, 0x8e, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x08, 0x00, 0x80, 0x02, 0x00, 0x00,
    ];

    let expected = AddressMessage {
        header: AddressHeader {
            family: AddressFamily::Inet6,
            prefix_len: 128,
            flags: AddressHeaderFlags::Permanent,
            scope: AddressScope::Host,
            index: 1,
        },
        attributes: vec![
            AddressAttribute::Address(IpAddr::V6(Ipv6Addr::LOCALHOST)),
            AddressAttribute::CacheInfo(CacheInfo {
                ifa_preferred: u32::MAX,
                ifa_valid: u32::MAX,
                cstamp: 142,
                tstamp: 142,
            }),
            AddressAttribute::Flags(
                AddressFlags::Permanent | AddressFlags::Noprefixroute,
            ),
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
