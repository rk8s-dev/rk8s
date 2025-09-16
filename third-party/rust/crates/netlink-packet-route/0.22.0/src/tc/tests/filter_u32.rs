// SPDX-License-Identifier: MIT

use std::net::Ipv4Addr;

use netlink_packet_utils::{Emitable, Parseable};

use crate::{
    tc::{
        filters::{TcU32OptionFlags, TcU32SelectorFlags},
        TcAttribute, TcFilterU32Option, TcHandle, TcHeader, TcMessage,
        TcMessageBuffer, TcOption, TcU32Key, TcU32Selector,
        TcU32SelectorBuffer,
    },
    AddressFamily,
};

// Setup:
//      ip link add veth1 type veth peer veth1.peer
//      ip link set veth1 up
//      ip link set veth1.peer up
//      tc qdisc add dev veth1 root handle 1: prio bands 4
//      tc qdisc add dev veth1 parent 1:4 handle 40: netem loss 10% delay 40ms
//      tc filter add dev veth1 \
//          protocol ip parent 1:0 prio 4 u32 match ip dst \
//          192.168.190.7 match ip dport 36000 0xffff flowid 1:4
//
// Capture nlmon of this command:
//
//      tc -s filter show dev veth1
//
// Raw packet modification:
//   * rtnetlink header removed.
#[test]
fn test_get_filter_u32() {
    let raw = vec![
        0x00, 0x00, 0x00, 0x00, 0x23, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x80,
        0x00, 0x00, 0x01, 0x00, 0x08, 0x00, 0x04, 0x00, 0x08, 0x00, 0x01, 0x00,
        0x75, 0x33, 0x32, 0x00, 0x08, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x74, 0x00, 0x02, 0x00, 0x34, 0x00, 0x05, 0x00, 0x01, 0x00, 0x02, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0xff, 0xff, 0xff, 0xff, 0xc0, 0xa8, 0xbe, 0x07, 0x10, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x8c, 0xa0,
        0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x02, 0x00,
        0x00, 0x00, 0x00, 0x80, 0x08, 0x00, 0x01, 0x00, 0x04, 0x00, 0x01, 0x00,
        0x08, 0x00, 0x0b, 0x00, 0x08, 0x00, 0x00, 0x00, 0x24, 0x00, 0x09, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let expected = TcMessage {
        header: TcHeader {
            family: AddressFamily::Unspec,
            index: 35,
            handle: TcHandle {
                major: 0x8000,
                minor: 0x800,
            },
            parent: TcHandle { major: 1, minor: 0 },
            info: 262152,
        },
        attributes: vec![
            TcAttribute::Kind("u32".to_string()),
            TcAttribute::Chain(0),
            TcAttribute::Options(vec![
                TcOption::U32(TcFilterU32Option::Selector(TcU32Selector {
                    flags: TcU32SelectorFlags::Terminal,
                    offshift: 0,
                    nkeys: 2,
                    offmask: 0,
                    off: 0,
                    offoff: 0,
                    hoff: 0,
                    hmask: 0,
                    keys: vec![
                        TcU32Key {
                            mask: 0xffffffff,
                            val: u32::from_ne_bytes(
                                Ipv4Addr::new(192, 168, 190, 7).octets(),
                            ),
                            off: 16,
                            offmask: 0,
                        },
                        TcU32Key {
                            mask: 0xffff0000,
                            val: u32::from_be(36000),
                            off: 20,
                            offmask: 0,
                        },
                    ],
                })),
                TcOption::U32(TcFilterU32Option::Hash(u32::from_be(0x80))),
                TcOption::U32(TcFilterU32Option::ClassId(TcHandle {
                    major: 1,
                    minor: 4,
                })),
                TcOption::U32(TcFilterU32Option::Flags(
                    TcU32OptionFlags::NotInHw,
                )),
                TcOption::U32(TcFilterU32Option::Pnct(vec![0; 32])), // TODO
            ]),
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

// Verify that [`TcU32Selector`] fails to parse a buffer with an
// invalid number of keys.
#[test]
fn test_tcu32_selector_invalid_nkeys() {
    // TC u32 selector buffer layout:
    // |byte0|byte1|byte2|byte3|byte4|byte5|byte6|byte7|
    // |-----|-----|-----|-----|-----|-----|-----|-----|
    // |flags|shift|nkeys|pad  |  offmask  |    off    |
    // |-----|-----|-----|-----|-----|-----|-----|-----|
    // |   offoff  |   hoff    |         hmask         |
    // |-----|-----|-----|-----|-----|-----|-----|-----|
    // |                     keys                      |
    // |                      ...                      |
    let buffer = [
        0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];
    assert!(TcU32SelectorBuffer::new_checked(buffer).is_err());
}
