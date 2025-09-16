// SPDX-License-Identifier: MIT

use netlink_packet_core::{
    NetlinkHeader, NetlinkMessage, NetlinkPayload, NLM_F_DUMP, NLM_F_REQUEST,
};
use netlink_packet_route::{
    neighbour::{NeighbourAddress, NeighbourAttribute, NeighbourMessage},
    AddressFamily, RouteNetlinkMessage,
};
use netlink_sys::{protocols::NETLINK_ROUTE, Socket, SocketAddr};

fn main() {
    let mut socket = Socket::new(NETLINK_ROUTE).unwrap();
    let _port_number = socket.bind_auto().unwrap().port_number();
    socket.connect(&SocketAddr::new(0, 0)).unwrap();

    let mut nl_hdr = NetlinkHeader::default();
    nl_hdr.flags = NLM_F_DUMP | NLM_F_REQUEST;

    let mut req = NetlinkMessage::new(
        nl_hdr,
        NetlinkPayload::from(RouteNetlinkMessage::GetNeighbour(
            NeighbourMessage::default(),
        )),
    );
    // IMPORTANT: call `finalize()` to automatically set the
    // `message_type` and `length` fields to the appropriate values in
    // the netlink header.
    req.finalize();

    let mut buf = vec![0; req.header.length as usize];
    req.serialize(&mut buf[..]);

    println!(">>> {req:?}");
    socket.send(&buf[..], 0).unwrap();

    let mut receive_buffer = vec![0; 4096];
    let mut offset = 0;

    'outer: loop {
        let size = socket.recv(&mut &mut receive_buffer[..], 0).unwrap();

        loop {
            let bytes = &receive_buffer[offset..];
            // Parse the message
            let msg: NetlinkMessage<RouteNetlinkMessage> =
                NetlinkMessage::deserialize(bytes).unwrap();

            match msg.payload {
                NetlinkPayload::Done(_) => break 'outer,
                NetlinkPayload::InnerMessage(
                    RouteNetlinkMessage::NewNeighbour(entry),
                ) => {
                    let address_family = entry.header.family;
                    if address_family == AddressFamily::Inet
                        || address_family == AddressFamily::Inet6
                    {
                        print_entry(entry);
                    }
                }
                NetlinkPayload::Error(err) => {
                    eprintln!("Received a netlink error message: {err:?}");
                    return;
                }
                _ => {}
            }

            offset += msg.header.length as usize;
            if offset == size || msg.header.length == 0 {
                offset = 0;
                break;
            }
        }
    }
}

fn format_ip(addr: &NeighbourAddress) -> String {
    if let NeighbourAddress::Inet(ip) = addr {
        ip.to_string()
    } else if let NeighbourAddress::Inet6(ip) = addr {
        ip.to_string()
    } else {
        panic!("Invalid IP Address");
    }
}

fn format_mac(buf: &[u8]) -> String {
    if buf.len() == 6 {
        format!(
            "{:<02x}:{:<02x}:{:<02x}:{:<02x}:{:<02x}:{:<02x}",
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5]
        )
    } else {
        "00:00:00:00:00:00".into()
    }
}

fn print_entry(entry: NeighbourMessage) {
    let state = entry.header.state;
    if let (Some(dest), Some(lladdr)) = (
        entry.attributes.iter().find_map(|nla| {
            if let NeighbourAttribute::Destination(addr) = nla {
                Some(format_ip(addr))
            } else {
                None
            }
        }),
        entry.attributes.iter().find_map(|nla| {
            if let NeighbourAttribute::LinkLocalAddress(addr) = nla {
                Some(format_mac(addr))
            } else {
                None
            }
        }),
    ) {
        println!("{dest:<30} {lladdr:<20} ({state})");
    }
}
