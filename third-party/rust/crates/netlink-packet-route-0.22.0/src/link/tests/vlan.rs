// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::link::link_flag::LinkFlags;
use crate::link::{
    InfoData, InfoKind, InfoVlan, LinkAttribute, LinkHeader, LinkInfo,
    LinkLayerType, LinkMessage, LinkMessageBuffer, VlanProtocol,
    VlanQosMapping,
};
use crate::AddressFamily;

#[test]
fn test_parsing_link_vlan() {
    let raw = vec![
        0x00, 0x00, 0x01, 0x00, 0x22, 0x00, 0x00, 0x00, 0x43, 0x10, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, // change flags 0
        0x50, 0x00, // length 80
        0x12, 0x00, // IFLA_LINKINFO 18
        0x09, 0x00, // length
        0x01, 0x00, // IFLA_INFO_KIND 1
        0x76, 0x6c, 0x61, 0x6e, 0x00, // 'vlan\0'
        0x00, 0x00, 0x00, // padding
        0x40, 0x00, // length 64
        0x02, 0x00, // IFLA_INFO_DATA 2
        0x06, 0x00, // length 06
        0x05, 0x00, // IFLA_VLAN_PROTOCOL 5
        0x81, 0x00, // big endian 0x8100 ETH_P_8021Q
        0x00, 0x00, // padding
        0x06, 0x00, // length 06
        0x01, 0x00, // IFLA_VLAN_ID 1
        0x65, 0x00, // VLAN ID 101
        0x00, 0x00, // padding
        0x0c, 0x00, // length 12
        0x02, 0x00, // IFLA_VLAN_FLAGS 2
        0x01, 0x00, 0x00, 0x00, // flags VLAN_FLAG_REORDER_HDR(1)
        0xff, 0xff, 0xff, 0xff, // mask
        0x10, 0x00, // length 16
        0x04, 0x00, // IFLA_VLAN_INGRESS_QOS 4
        0x0c, 0x00, // length 12
        0x01, 0x00, // IFLA_VLAN_QOS_MAPPING 1
        0x06, 0x00, 0x00, 0x00, // from 6
        0x07, 0x00, 0x00, 0x00, // to 7
        0x10, 0x00, // length 16
        0x03, 0x00, // IFLA_VLAN_EGRESS_QOS 3
        0x0c, 0x00, // length 12
        0x01, 0x00, // IFLA_VLAN_QOS_MAPPING 1
        0x04, 0x00, 0x00, 0x00, // from 4
        0x05, 0x00, 0x00, 0x00, // to 5
    ];

    let expected = LinkMessage {
        header: LinkHeader {
            interface_family: AddressFamily::Unspec,
            index: 34,
            link_layer_type: LinkLayerType::Ether,
            flags: LinkFlags::Broadcast
                | LinkFlags::LowerUp
                | LinkFlags::Multicast
                | LinkFlags::Running
                | LinkFlags::Up,
            change_mask: LinkFlags::empty(),
        },
        attributes: vec![LinkAttribute::LinkInfo(vec![
            LinkInfo::Kind(InfoKind::Vlan),
            LinkInfo::Data(InfoData::Vlan(vec![
                InfoVlan::Protocol(VlanProtocol::Ieee8021Q),
                InfoVlan::Id(101),
                InfoVlan::Flags((1, 4294967295)),
                InfoVlan::IngressQos(vec![VlanQosMapping::Mapping(6, 7)]),
                InfoVlan::EgressQos(vec![VlanQosMapping::Mapping(4, 5)]),
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
