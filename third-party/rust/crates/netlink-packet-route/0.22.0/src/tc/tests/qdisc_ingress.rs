// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::{
    tc::{
        TcAttribute, TcHandle, TcHeader, TcMessage, TcMessageBuffer, TcStats,
        TcStats2, TcStatsBasic, TcStatsQueue,
    },
    AddressFamily,
};

// Setup:
//      ip link add veth1 type veth peer veth1.peer
//      ip link set veth1 up
//      ip link set veth1.peer up
//      tc qdisc add dev veth1 handle ffff: ingress
//
// Capture nlmon of this command:
//
//      tc -s qdisc show dev veth1
//
// Raw packet modification:
//   * rtnetlink header removed.
#[test]
fn test_get_qdisc_ingress() {
    let raw = vec![
        0x00, 0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff,
        0xf1, 0xff, 0xff, 0xff, 0x01, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x01, 0x00,
        0x69, 0x6e, 0x67, 0x72, 0x65, 0x73, 0x73, 0x00, 0x04, 0x00, 0x02, 0x00,
        0x05, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x30, 0x00, 0x07, 0x00,
        0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00, 0x03, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x03, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    let expected = TcMessage {
        header: TcHeader {
            family: AddressFamily::Unspec,
            index: 31,
            handle: TcHandle {
                major: 0xffff,
                minor: 0,
            },
            parent: TcHandle {
                major: 0xffff,
                minor: 0xfff1,
            },
            info: 1,
        },
        attributes: vec![
            TcAttribute::Kind("ingress".to_string()),
            TcAttribute::Options(vec![]),
            TcAttribute::HwOffload(0),
            TcAttribute::Stats2(vec![
                TcStats2::Basic(TcStatsBasic {
                    bytes: 0,
                    packets: 0,
                }),
                TcStats2::Queue(TcStatsQueue {
                    qlen: 0,
                    backlog: 0,
                    drops: 0,
                    requeues: 0,
                    overlimits: 0,
                }),
            ]),
            TcAttribute::Stats(TcStats {
                bytes: 0,
                packets: 0,
                drops: 0,
                overlimits: 0,
                bps: 0,
                pps: 0,
                qlen: 0,
                backlog: 0,
            }),
        ],
    };

    assert_eq!(
        expected,
        TcMessage::parse(&TcMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}
