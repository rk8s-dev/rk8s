// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::link::link_flag::LinkFlags;
use crate::link::{
    InfoData, InfoKind, InfoVeth, LinkAttribute, LinkHeader, LinkInfo,
    LinkLayerType, LinkMessage, LinkMessageBuffer,
};
use crate::AddressFamily;

#[test]
fn test_veth_get_link_info() {
    let raw: Vec<u8> = vec![
        0x00, 0x00, // AF_UNSPEC and reserved
        0x01, 0x00, // Link layer type ethernet(1)
        0x19, 0x00, 0x00, 0x00, // iface index 25
        0x43, 0x10, 0x01, 0x00, // flags
        0x00, 0x00, 0x00, 0x00, // changed flags0
        0x10, 0x00, // length 16
        0x12, 0x00, // IFLA_LINKINFO 18
        0x09, 0x00, // length 09
        0x01, 0x00, // IFLA_INFO_KIND
        0x76, 0x65, 0x74, 0x68, 0x00, // 'bond\0'
        0x00, 0x00, 0x00, // padding
    ];

    let expected = LinkMessage {
        header: LinkHeader {
            interface_family: AddressFamily::Unspec,
            index: 25,
            link_layer_type: LinkLayerType::Ether,
            flags: LinkFlags::Broadcast
                | LinkFlags::LowerUp
                | LinkFlags::Multicast
                | LinkFlags::Running
                | LinkFlags::Up,
            change_mask: LinkFlags::empty(),
        },
        attributes: vec![LinkAttribute::LinkInfo(vec![LinkInfo::Kind(
            InfoKind::Veth,
        )])],
    };

    assert_eq!(
        expected,
        LinkMessage::parse(&LinkMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}

#[test]
fn test_crate_veth() {
    // With `iproute 6.5.0`, the IFLA_INFO_KIND will not use NULL terminated
    // string.
    // This is bug of iproute: https://issues.redhat.com/browse/RHEL-14964
    // The correct way is NULL terminated `IFLA_INFO_KIND`, hence below packet
    // is different from what iproute generate.
    let raw: Vec<u8> = vec![
        0x00, // interface family AF_UNSPEC
        0x00, // reserved
        0x00, 0x00, // link layer type 0
        0x00, 0x00, 0x00, 0x00, // iface index 0
        0x00, 0x00, 0x00, 0x00, // device flags 0
        0x00, 0x00, 0x00, 0x00, // change flags 0
        0x0a, 0x00, // length 10
        0x03, 0x00, // IFLA_IFNAME
        0x76, 0x65, 0x74, 0x68, 0x31, 0x00, 0x00, 0x00, // "veth0\0"
        0x38, 0x00, // length 56
        0x12, 0x00, // IFLA_LINKINFO 18
        0x09, 0x00, // length 9
        0x01, 0x00, // IFLA_INFO_KIND 1
        0x76, 0x65, 0x74, 0x68, 0x00, // 'veth\0'
        0x00, 0x00, 0x00, // padding
        0x28, 0x00, // length 40
        0x02, 0x00, // IFLA_INFO_DATA 2
        0x24, 0x00, // length 36
        0x01, 0x00, // VETH_INFO_PEER 1
        0x00, // Netlink Message header: family AF_UNSPEC
        0x00, // reserved
        0x00, 0x00, // link layer type 0
        0x00, 0x00, 0x00, 0x00, // iface index 0
        0x00, 0x00, 0x00, 0x00, // device flags 0
        0x00, 0x00, 0x00, 0x00, // change flags 0
        0x0d, 0x00, // length 16
        0x03, 0x00, // IFLA_IFNAME 3
        0x76, 0x65, 0x74, 0x68, 0x31, 0x2d, 0x65, 0x70,
        0x00, // "veth1-ep\0"
        0x00, 0x00, 0x00, // padding
    ];

    let expected = LinkMessage {
        attributes: vec![
            LinkAttribute::IfName("veth1".to_string()),
            LinkAttribute::LinkInfo(vec![
                LinkInfo::Kind(InfoKind::Veth),
                LinkInfo::Data(InfoData::Veth(InfoVeth::Peer(LinkMessage {
                    attributes: vec![LinkAttribute::IfName(
                        "veth1-ep".to_string(),
                    )],
                    ..Default::default()
                }))),
            ]),
        ],
        ..Default::default()
    };

    assert_eq!(
        expected,
        LinkMessage::parse(&LinkMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}
