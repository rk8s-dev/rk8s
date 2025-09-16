// SPDX-License-Identifier: MIT

// This file only contains testing parsing RouteNetlinkMessage, not focusing on
// detailed sub-component parsing. Each component has their own tests moduel.

use netlink_packet_core::{NetlinkHeader, NetlinkMessage, NetlinkPayload};
use netlink_packet_utils::Emitable;

use crate::{
    link::{LinkAttribute, LinkExtentMask, LinkMessage},
    RouteNetlinkMessage,
};

// wireshark capture of nlmon against command:
//   ip link show dev lo
#[test]
fn test_get_link() {
    let raw: Vec<u8> = vec![
        0x30, 0x00, 0x00, 0x00, 0x12, 0x00, 0x01, 0x00, 0xe6, 0x9c, 0x69, 0x65,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x1d, 0x00,
        0x09, 0x00, 0x00, 0x00, 0x07, 0x00, 0x03, 0x00, 0x6c, 0x6f, 0x00, 0x00,
    ];

    let mut header = NetlinkHeader::default();
    header.length = 48;
    header.message_type = 18;
    header.flags = 1;
    header.sequence_number = 1701420262;

    let expected = NetlinkMessage::new(
        header,
        NetlinkPayload::from(RouteNetlinkMessage::GetLink(LinkMessage {
            attributes: vec![
                LinkAttribute::ExtMask(vec![
                    LinkExtentMask::Vf,
                    LinkExtentMask::SkipStats,
                ]),
                LinkAttribute::IfName("lo".to_string()),
            ],
            ..Default::default()
        })),
    );

    assert_eq!(NetlinkMessage::deserialize(&raw).unwrap(), expected);
    let mut buffer = vec![0; expected.buffer_len()];
    expected.emit(&mut buffer);
    assert_eq!(buffer.as_slice(), raw);
}
