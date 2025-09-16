// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::link::link_flag::LinkFlags;
use crate::link::{
    LinkAttribute, LinkHeader, LinkLayerType, LinkMessage, LinkMessageBuffer,
    Prop,
};
use crate::AddressFamily;

#[test]
fn test_wlan0_with_prop_altname() {
    // nlmon dump of `ip link show wlan0` with two alt_name for wlan0 with
    // IFLA_PROP_LIST only
    let raw = vec![
        0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x43, 0x10, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x34, 0x80, 0x0e, 0x00, 0x35, 0x00,
        0x77, 0x6c, 0x70, 0x30, 0x73, 0x32, 0x30, 0x66, 0x33, 0x00, 0x00, 0x00,
        0x09, 0x00, 0x35, 0x00, 0x77, 0x69, 0x66, 0x69, 0x00, 0x00, 0x00, 0x00,
    ];

    let expected = LinkMessage {
        header: LinkHeader {
            interface_family: AddressFamily::Unspec,
            index: 2,
            link_layer_type: LinkLayerType::Ether,
            flags: LinkFlags::Broadcast
                | LinkFlags::LowerUp
                | LinkFlags::Multicast
                | LinkFlags::Running
                | LinkFlags::Up,
            change_mask: LinkFlags::empty(),
        },
        attributes: vec![LinkAttribute::PropList(vec![
            Prop::AltIfName("wlp0s20f3".to_string()),
            Prop::AltIfName("wifi".to_string()),
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
