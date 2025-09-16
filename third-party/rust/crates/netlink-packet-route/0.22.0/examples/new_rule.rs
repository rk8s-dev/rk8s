// SPDX-License-Identifier: MIT

use netlink_packet_core::{
    NetlinkHeader, NetlinkMessage, NetlinkPayload, NLM_F_ACK, NLM_F_CREATE,
    NLM_F_EXCL, NLM_F_REQUEST,
};
use netlink_packet_route::{
    route::RouteProtocol,
    rule::{RuleAction, RuleAttribute, RuleHeader, RuleMessage},
    AddressFamily, RouteNetlinkMessage,
};
use netlink_sys::{protocols::NETLINK_ROUTE, Socket, SocketAddr};

fn main() {
    let mut socket = Socket::new(NETLINK_ROUTE).unwrap();
    let _port_number = socket.bind_auto().unwrap().port_number();
    socket.connect(&SocketAddr::new(0, 0)).unwrap();

    let rule_msg_hdr = RuleHeader {
        family: AddressFamily::Inet,
        table: 254,
        action: RuleAction::ToTable,
        ..Default::default()
    };

    let mut rule_msg = RuleMessage::default();
    rule_msg.header = rule_msg_hdr;
    rule_msg.attributes = vec![
        RuleAttribute::Table(254),
        RuleAttribute::SuppressPrefixLen(4294967295),
        RuleAttribute::Priority(1000),
        RuleAttribute::Protocol(RouteProtocol::Kernel),
    ];
    let mut nl_hdr = NetlinkHeader::default();
    nl_hdr.flags = NLM_F_REQUEST | NLM_F_CREATE | NLM_F_EXCL | NLM_F_ACK;

    let mut msg = NetlinkMessage::new(
        nl_hdr,
        NetlinkPayload::from(RouteNetlinkMessage::NewRule(rule_msg)),
    );

    msg.finalize();
    let mut buf = vec![0; 1024 * 8];

    msg.serialize(&mut buf[..msg.buffer_len()]);

    println!(">>> {msg:?}");

    socket
        .send(&buf, 0)
        .expect("failed to send netlink message");

    let mut receive_buffer = vec![0; 4096];

    while let Ok(_size) = socket.recv(&mut receive_buffer, 0) {
        let bytes = &receive_buffer[..];
        let rx_packet =
            <NetlinkMessage<RouteNetlinkMessage>>::deserialize(bytes);
        println!("<<< {rx_packet:?}");
        if let Ok(rx_packet) = rx_packet {
            if let NetlinkPayload::Error(e) = rx_packet.payload {
                eprintln!("{e:?}");
            } else {
                return;
            }
        }
    }
}
