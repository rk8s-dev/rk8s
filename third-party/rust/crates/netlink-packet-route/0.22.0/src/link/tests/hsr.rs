// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::link::link_flag::LinkFlags;
use crate::link::{
    HsrProtocol, InfoData, InfoHsr, InfoKind, LinkAttribute, LinkHeader,
    LinkInfo, LinkLayerType, LinkMessage, LinkMessageBuffer,
};
use crate::AddressFamily;

#[test]
fn test_parsing_link_hsr() {
    let raw = vec![
        0x00, 0x00, 0x01, 0x00, 0x2d, 0x00, 0x00, 0x00, 0x43, 0x10, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x12, 0x00, 0x08, 0x00, 0x01, 0x00,
        0x68, 0x73, 0x72, 0x00, 0x30, 0x00, 0x02, 0x00, 0x08, 0x00, 0x01, 0x00,
        0x2c, 0x00, 0x00, 0x00, 0x08, 0x00, 0x02, 0x00, 0x2a, 0x00, 0x00, 0x00,
        0x0a, 0x00, 0x04, 0x00, 0x01, 0x15, 0x4e, 0x00, 0x01, 0x90, 0x00, 0x00,
        0x06, 0x00, 0x05, 0x00, 0x09, 0xfc, 0x00, 0x00, 0x05, 0x00, 0x07, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    let expected = LinkMessage {
        header: LinkHeader {
            interface_family: AddressFamily::Unspec,
            index: 45,
            link_layer_type: LinkLayerType::Ether,
            flags: LinkFlags::Broadcast
                | LinkFlags::LowerUp
                | LinkFlags::Multicast
                | LinkFlags::Running
                | LinkFlags::Up,
            change_mask: LinkFlags::empty(),
        },
        attributes: vec![LinkAttribute::LinkInfo(vec![
            LinkInfo::Kind(InfoKind::Hsr),
            LinkInfo::Data(InfoData::Hsr(vec![
                InfoHsr::Port1(44),
                InfoHsr::Port2(42),
                InfoHsr::SupervisionAddr([0x01, 0x15, 0x4e, 0x00, 0x01, 0x90]),
                InfoHsr::SeqNr(64521),
                InfoHsr::Protocol(HsrProtocol::Hsr),
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
