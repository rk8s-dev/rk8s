// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::link::link_flag::LinkFlags;
use crate::link::{
    InfoData, InfoKind, InfoXfrm, LinkAttribute, LinkHeader, LinkInfo,
    LinkLayerType, LinkMessage, LinkMessageBuffer,
};
use crate::AddressFamily;

#[test]
fn test_parsing_link_xfrm() {
    let raw = vec![
        0x00, 0x00, 0xfe, 0xff, 0x28, 0x00, 0x00, 0x00, 0xc1, 0x00, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x24, 0x00, 0x12, 0x00, 0x09, 0x00, 0x01, 0x00,
        0x78, 0x66, 0x72, 0x6d, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x02, 0x00,
        0x08, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x08, 0x00, 0x02, 0x00,
        0x0a, 0x00, 0x00, 0x00,
    ];

    let expected = LinkMessage {
        header: LinkHeader {
            interface_family: AddressFamily::Unspec,
            index: 40,
            link_layer_type: LinkLayerType::None,
            flags: LinkFlags::LowerUp
                | LinkFlags::Noarp
                | LinkFlags::Running
                | LinkFlags::Up,
            change_mask: LinkFlags::empty(),
        },
        attributes: vec![LinkAttribute::LinkInfo(vec![
            LinkInfo::Kind(InfoKind::Xfrm),
            LinkInfo::Data(InfoData::Xfrm(vec![
                InfoXfrm::Link(2),
                InfoXfrm::IfId(10),
            ])),
        ])],
    };

    assert_eq!(
        expected,
        LinkMessage::parse(&LinkMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}
