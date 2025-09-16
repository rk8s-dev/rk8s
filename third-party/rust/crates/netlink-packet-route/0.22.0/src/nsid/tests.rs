// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::{
    nsid::{NsidAttribute, NsidHeader, NsidMessage, NsidMessageBuffer},
    AddressFamily,
};

// Setup
//      ip netns add abc
//      ip netns set abc 99
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip netns list
#[test]
fn test_ip_netns_query_reply() {
    let raw = vec![
        0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x00, 0x63, 0x00, 0x00, 0x00,
    ];

    let expected = NsidMessage {
        header: NsidHeader {
            family: AddressFamily::Unspec,
        },
        attributes: vec![NsidAttribute::Id(99)],
    };

    assert_eq!(
        expected,
        NsidMessage::parse(&NsidMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}

// Setup
//      ip netns add abc
//      ip netns set abc 99
// wireshark capture(netlink message header removed) of nlmon against command:
//      ip netns list
#[test]
fn test_ip_netns_query() {
    let raw = vec![
        0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x03, 0x00, 0x06, 0x00, 0x00, 0x00,
    ];

    let expected = NsidMessage {
        header: NsidHeader {
            family: AddressFamily::Unspec,
        },
        attributes: vec![NsidAttribute::Fd(6)],
    };

    assert_eq!(
        expected,
        NsidMessage::parse(&NsidMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}

// wireshark capture(netlink message header removed) of nlmon against command:
//      ip netns list-id target-nsid 99
#[test]
fn test_ip_netns_query_target_ns_id() {
    let raw = vec![
        0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x04, 0x00, 0x63, 0x00, 0x00, 0x00,
    ];

    let expected = NsidMessage {
        header: NsidHeader {
            family: AddressFamily::Unspec,
        },
        attributes: vec![NsidAttribute::TargetNsid(99)],
    };

    assert_eq!(
        expected,
        NsidMessage::parse(&NsidMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}
