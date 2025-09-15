// SPDX-License-Identifier: MIT

use netlink_packet_utils::traits::{Emitable, ParseableParametrized};

use crate::link::link_flag::LinkFlags;
use crate::link::{
    LinkAttribute, LinkHeader, LinkLayerType, LinkMessage, LinkMessageBuffer,
    State,
};
use crate::AddressFamily;

static LINK_MSG: [u8; 96] = [
    0x00, // interface family AF_UNSPEC
    0x00, // reserved
    0x04, 0x03, // link layer type 772 = loopback
    0x01, 0x00, 0x00, 0x00, // interface index = 1
    0x49, 0x00, 0x01, 0x00, // flags: UP|LOOPBACK|RUNNING|LOWERUP
    0x00, 0x00, 0x00, 0x00, // reserved 2 (aka device change flag)
    // attributes
    0x07, 0x00, 0x03, 0x00, 0x6c, 0x6f, 0x00, // device name L=7,T=3,V=lo
    0x00, // padding
    0x08, 0x00, 0x0d, 0x00, 0xe8, 0x03, 0x00,
    0x00, // TxQueue length L=8,T=13,V=1000
    0x05, 0x00, 0x10, 0x00, 0x00, // OperState L=5,T=16,V=0 (unknown)
    0x00, 0x00, 0x00, // padding
    0x05, 0x00, 0x11, 0x00, 0x00, // Link mode L=5,T=17,V=0
    0x00, 0x00, 0x00, // padding
    0x08, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00, // MTU L=8,T=4,V=65536
    0x08, 0x00, 0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, // Group L=8,T=27,V=9
    0x08, 0x00, 0x1e, 0x00, 0x00, 0x00, 0x00,
    0x00, // Promiscuity L=8,T=30,V=0
    0x08, 0x00, 0x1f, 0x00, 0x01, 0x00, 0x00,
    0x00, // Number of Tx Queues L=8,T=31,V=1
    0x08, 0x00, 0x28, 0x00, 0xff, 0xff, 0x00,
    0x00, // Maximum GSO segment count L=8,T=40,V=65536
    0x08, 0x00, 0x29, 0x00, 0x00, 0x00, 0x01,
    0x00, // Maximum GSO size L=8,T=41,V=65536
];

#[test]
fn link_message_packet_header_read() {
    let packet = LinkMessageBuffer::new(&LINK_MSG[0..16]);
    assert_eq!(packet.interface_family(), AddressFamily::Unspec.into());
    assert_eq!(packet.reserved_1(), 0);
    assert_eq!(packet.link_layer_type(), LinkLayerType::Loopback.into());
    assert_eq!(packet.link_index(), 1);
    assert_eq!(
        packet.flags(),
        (LinkFlags::Loopback
            | LinkFlags::LowerUp
            | LinkFlags::Running
            | LinkFlags::Up)
            .bits()
    );
    assert_eq!(packet.change_mask(), 0);
}

#[test]
fn link_message_packet_header_build() {
    let mut buf = vec![0xff; 16];
    {
        let mut packet = LinkMessageBuffer::new(&mut buf);
        packet.set_interface_family(AddressFamily::Unspec.into());
        packet.set_reserved_1(0);
        packet.set_link_layer_type(LinkLayerType::Loopback.into());
        packet.set_link_index(1);
        packet.set_flags(
            (LinkFlags::Loopback
                | LinkFlags::LowerUp
                | LinkFlags::Running
                | LinkFlags::Up)
                .bits(),
        );
        packet.set_change_mask(0);
    }
    assert_eq!(&buf[..], &LINK_MSG[0..16]);
}

#[test]
fn link_mssage_packet_attributes_read() {
    let packet = LinkMessageBuffer::new(&LINK_MSG[..]);
    assert_eq!(packet.attributes().count(), 10);
    let mut attributes = packet.attributes();

    // device name L=7,T=3,V=lo
    let nla = attributes.next().unwrap().unwrap();
    nla.check_buffer_length().unwrap();
    assert_eq!(nla.length(), 7);
    assert_eq!(nla.kind(), 3);
    assert_eq!(nla.value(), &[0x6c, 0x6f, 0x00]);
    let parsed =
        LinkAttribute::parse_with_param(&nla, AddressFamily::Inet).unwrap();
    assert_eq!(parsed, LinkAttribute::IfName(String::from("lo")));

    // TxQueue length L=8,T=13,V=1000
    let nla = attributes.next().unwrap().unwrap();
    nla.check_buffer_length().unwrap();
    assert_eq!(nla.length(), 8);
    assert_eq!(nla.kind(), 13);
    assert_eq!(nla.value(), &[0xe8, 0x03, 0x00, 0x00]);
    let parsed =
        LinkAttribute::parse_with_param(&nla, AddressFamily::Inet).unwrap();
    assert_eq!(parsed, LinkAttribute::TxQueueLen(1000));

    // OperState L=5,T=16,V=0 (unknown)
    let nla = attributes.next().unwrap().unwrap();
    nla.check_buffer_length().unwrap();
    assert_eq!(nla.length(), 5);
    assert_eq!(nla.kind(), 16);
    assert_eq!(nla.value(), &[0x00]);
    let parsed =
        LinkAttribute::parse_with_param(&nla, AddressFamily::Inet).unwrap();
    assert_eq!(parsed, LinkAttribute::OperState(State::Unknown));

    // Link mode L=5,T=17,V=0
    let nla = attributes.next().unwrap().unwrap();
    nla.check_buffer_length().unwrap();
    assert_eq!(nla.length(), 5);
    assert_eq!(nla.kind(), 17);
    assert_eq!(nla.value(), &[0x00]);
    let parsed =
        LinkAttribute::parse_with_param(&nla, AddressFamily::Inet).unwrap();
    assert_eq!(parsed, LinkAttribute::Mode(0));

    // MTU L=8,T=4,V=65536
    let nla = attributes.next().unwrap().unwrap();
    nla.check_buffer_length().unwrap();
    assert_eq!(nla.length(), 8);
    assert_eq!(nla.kind(), 4);
    assert_eq!(nla.value(), &[0x00, 0x00, 0x01, 0x00]);
    let parsed =
        LinkAttribute::parse_with_param(&nla, AddressFamily::Inet).unwrap();
    assert_eq!(parsed, LinkAttribute::Mtu(65_536));

    // 0x00, 0x00, 0x00, 0x00,
    // Group L=8,T=27,V=9
    let nla = attributes.next().unwrap().unwrap();
    nla.check_buffer_length().unwrap();
    assert_eq!(nla.length(), 8);
    assert_eq!(nla.kind(), 27);
    assert_eq!(nla.value(), &[0x00, 0x00, 0x00, 0x00]);
    let parsed =
        LinkAttribute::parse_with_param(&nla, AddressFamily::Inet).unwrap();
    assert_eq!(parsed, LinkAttribute::Group(0));

    // Promiscuity L=8,T=30,V=0
    let nla = attributes.next().unwrap().unwrap();
    nla.check_buffer_length().unwrap();
    assert_eq!(nla.length(), 8);
    assert_eq!(nla.kind(), 30);
    assert_eq!(nla.value(), &[0x00, 0x00, 0x00, 0x00]);
    let parsed =
        LinkAttribute::parse_with_param(&nla, AddressFamily::Inet).unwrap();
    assert_eq!(parsed, LinkAttribute::Promiscuity(0));

    // Number of Tx Queues L=8,T=31,V=1
    // 0x01, 0x00, 0x00, 0x00
    let nla = attributes.next().unwrap().unwrap();
    nla.check_buffer_length().unwrap();
    assert_eq!(nla.length(), 8);
    assert_eq!(nla.kind(), 31);
    assert_eq!(nla.value(), &[0x01, 0x00, 0x00, 0x00]);
    let parsed =
        LinkAttribute::parse_with_param(&nla, AddressFamily::Inet).unwrap();
    assert_eq!(parsed, LinkAttribute::NumTxQueues(1));
}

#[test]
fn link_message_emit() {
    let header = LinkHeader {
        link_layer_type: LinkLayerType::Loopback,
        index: 1,
        flags: LinkFlags::Loopback
            | LinkFlags::LowerUp
            | LinkFlags::Running
            | LinkFlags::Up,
        interface_family: AddressFamily::Unspec,
        ..Default::default()
    };

    let attributes = vec![
        LinkAttribute::IfName("lo".into()),
        LinkAttribute::TxQueueLen(1000),
        LinkAttribute::OperState(State::Unknown),
        LinkAttribute::Mode(0),
        LinkAttribute::Mtu(0x1_0000),
        LinkAttribute::Group(0),
        LinkAttribute::Promiscuity(0),
        LinkAttribute::NumTxQueues(1),
        LinkAttribute::GsoMaxSegs(0xffff),
        LinkAttribute::GsoMaxSize(0x1_0000),
    ];

    let packet = LinkMessage { header, attributes };

    let mut buf = [0; 96];

    assert_eq!(packet.buffer_len(), 96);
    packet.emit(&mut buf[..]);

    assert_eq!(buf, &LINK_MSG[..96]);
}
