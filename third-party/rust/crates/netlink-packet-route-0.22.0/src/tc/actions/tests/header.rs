// SPDX-License-Identifier: MIT

use crate::tc::actions::{TcActionMessageBuffer, TcActionMessageHeader};
use crate::AddressFamily;
use netlink_packet_utils::{Emitable, Parseable};

#[test]
fn tc_action_message_header_parse_back_all_known_families() {
    for family in [
        AddressFamily::Unspec,
        // AddressFamily::Local, // `Local` and `Unix` overlap!
        AddressFamily::Unix,
        AddressFamily::Inet,
        AddressFamily::Ax25,
        AddressFamily::Ipx,
        AddressFamily::Appletalk,
        AddressFamily::Netrom,
        AddressFamily::Bridge,
        AddressFamily::Atmpvc,
        AddressFamily::X25,
        AddressFamily::Inet6,
        AddressFamily::Rose,
        AddressFamily::Decnet,
        AddressFamily::Netbeui,
        AddressFamily::Security,
        AddressFamily::Key,
        // AddressFamily::Route, // `Route` and `Netlink` overlap!
        AddressFamily::Netlink,
        AddressFamily::Packet,
        AddressFamily::Ash,
        AddressFamily::Econet,
        AddressFamily::Atmsvc,
        AddressFamily::Rds,
        AddressFamily::Sna,
        AddressFamily::Irda,
        AddressFamily::Pppox,
        AddressFamily::Wanpipe,
        AddressFamily::Llc,
        AddressFamily::Ib,
        AddressFamily::Mpls,
        AddressFamily::Can,
        AddressFamily::Tipc,
        AddressFamily::Bluetooth,
        AddressFamily::Iucv,
        AddressFamily::Rxrpc,
        AddressFamily::Isdn,
        AddressFamily::Phonet,
        AddressFamily::Ieee802154,
        AddressFamily::Caif,
        AddressFamily::Alg,
        AddressFamily::Nfc,
        AddressFamily::Vsock,
        AddressFamily::Kcm,
        AddressFamily::Qipcrtr,
        AddressFamily::Smc,
        AddressFamily::Xdp,
        AddressFamily::Mctp,
    ] {
        let orig = TcActionMessageHeader { family };
        let mut buffer = vec![0; orig.buffer_len()];
        orig.emit(&mut buffer);
        let parsed = TcActionMessageHeader::parse(
            &TcActionMessageBuffer::new_checked(&buffer).unwrap(),
        )
        .unwrap();
        assert_eq!(orig, parsed);
    }
}

#[test]
fn tc_action_message_header_parse_back_other() {
    let orig = TcActionMessageHeader {
        family: AddressFamily::Other(99),
    };
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcActionMessageHeader::parse(
        &TcActionMessageBuffer::new_checked(&buffer).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}
