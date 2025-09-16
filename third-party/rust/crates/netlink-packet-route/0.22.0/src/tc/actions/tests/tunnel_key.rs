// SPDX-License-Identifier: MIT

use netlink_packet_utils::nla::NlaBuffer;
use netlink_packet_utils::{Emitable, Parseable};

use crate::tc::{
    TcAction, TcActionAttribute, TcActionGeneric, TcActionOption,
    TcActionTunnelKeyOption, TcActionType, TcStats2, TcStatsBasic,
    TcStatsQueue, TcTunnelKey, Tcf,
};
use std::net::{Ipv4Addr, Ipv6Addr};

//      > tc actions add action tunnel_key set id 33 src_ip 1.2.3.4 dst_ip
//      > 2.3.4.5 dst_port 4789 tos 1 ttl 2
//      > tools/nl_dump.py dump_actions tunnel_key
//      Note: 5.15 and 6.8 kernels do NOT set NLA_F_NESTED for TCA_ACT_OPTIONS
#[test]
fn get_tunnel_key_vxlan_action_ipv4() {
    let raw = vec![
        0xD4, 0x00, 0x01, 0x00, 0x0F, 0x00, 0x01, 0x00, 0x74, 0x75, 0x6E, 0x6E,
        0x65, 0x6C, 0x5F, 0x6B, 0x65, 0x79, 0x00, 0x00, 0x44, 0x00, 0x04, 0x00,
        0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x07, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x18, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x7C, 0x00, 0x02, 0x80, 0x1C, 0x00, 0x02, 0x00,
        0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x21, 0x08, 0x00, 0x03, 0x00,
        0x01, 0x02, 0x03, 0x04, 0x08, 0x00, 0x04, 0x00, 0x02, 0x03, 0x04, 0x05,
        0x06, 0x00, 0x09, 0x00, 0x12, 0xB5, 0x00, 0x00, 0x05, 0x00, 0x0A, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x0C, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x05, 0x00, 0x0D, 0x00, 0x02, 0x00, 0x00, 0x00, 0x24, 0x00, 0x01, 0x00,
        0xB0, 0x23, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xB0, 0x23, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let expected = TcAction {
        tab: 1,
        attributes: vec![
            TcActionAttribute::Kind("tunnel_key".to_string()),
            TcActionAttribute::Stats(vec![
                TcStats2::Basic(TcStatsBasic {
                    bytes: 0,
                    packets: 0,
                }),
                TcStats2::BasicHw(TcStatsBasic {
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
            TcActionAttribute::Options(vec![
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::Parms(
                    TcTunnelKey {
                        generic: TcActionGeneric {
                            index: 2,
                            capab: 0,
                            action: TcActionType::Pipe,
                            refcnt: 1,
                            bindcnt: 0,
                        },
                        t_action: 1,
                    },
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncKeyId(
                    33,
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncIpv4Src(
                    "1.2.3.4".parse::<Ipv4Addr>().unwrap(),
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncIpv4Dst(
                    "2.3.4.5".parse::<Ipv4Addr>().unwrap(),
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncDstPort(
                    4789,
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::NoCsum(
                    false,
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncTos(1)),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncTtl(2)),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::Tm(Tcf {
                    install: 9136,
                    lastuse: 9136,
                    expires: 0,
                    firstuse: 0,
                })),
            ]),
        ],
    };

    assert_eq!(expected, TcAction::parse(&NlaBuffer::new(&raw)).unwrap());

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}

//      > tc actions add action tunnel_key set id 33 src_ip 2a00:1:: dst_ip
//      > 2a01:2:: dst_port 4789 tos 1 ttl 2
//      > tools/nl_dump.py dump_actions tunnel_key
//      Note: 5.15 and 6.8 kernels do NOT set NLA_F_NESTED for TCA_ACT_OPTIONS
#[test]
fn test_action_tunnel_key_vxlan_ipv6() {
    let raw = vec![
        0xEC, 0x00, 0x00, 0x00, 0x0F, 0x00, 0x01, 0x00, 0x74, 0x75, 0x6E, 0x6E,
        0x65, 0x6C, 0x5F, 0x6B, 0x65, 0x79, 0x00, 0x00, 0x44, 0x00, 0x04, 0x00,
        0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x07, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x18, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x94, 0x00, 0x02, 0x80, 0x1C, 0x00, 0x02, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x08, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x21, 0x14, 0x00, 0x05, 0x00,
        0x2A, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x06, 0x00, 0x2A, 0x01, 0x00, 0x02,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x06, 0x00, 0x09, 0x00, 0x12, 0xB5, 0x00, 0x00, 0x05, 0x00, 0x0A, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x0C, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x05, 0x00, 0x0D, 0x00, 0x02, 0x00, 0x00, 0x00, 0x24, 0x00, 0x01, 0x00,
        0xFB, 0x71, 0x3A, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFB, 0x71, 0x3A, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let expected = TcAction {
        tab: 0,
        attributes: vec![
            TcActionAttribute::Kind("tunnel_key".to_string()),
            TcActionAttribute::Stats(vec![
                TcStats2::Basic(TcStatsBasic {
                    bytes: 0,
                    packets: 0,
                }),
                TcStats2::BasicHw(TcStatsBasic {
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
            TcActionAttribute::Options(vec![
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::Parms(
                    TcTunnelKey {
                        generic: TcActionGeneric {
                            index: 1,
                            capab: 0,
                            action: TcActionType::Pipe,
                            refcnt: 1,
                            bindcnt: 1,
                        },
                        t_action: 1,
                    },
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncKeyId(
                    33,
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncIpv6Src(
                    "2a00:1::".parse::<Ipv6Addr>().unwrap(),
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncIpv6Dst(
                    "2a01:2::".parse::<Ipv6Addr>().unwrap(),
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncDstPort(
                    4789,
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::NoCsum(
                    false,
                )),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncTos(1)),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::EncTtl(2)),
                TcActionOption::TunnelKey(TcActionTunnelKeyOption::Tm(Tcf {
                    install: 3830267,
                    lastuse: 3830267,
                    expires: 0,
                    firstuse: 0,
                })),
            ]),
        ],
    };

    assert_eq!(expected, TcAction::parse(&NlaBuffer::new(&raw)).unwrap());

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}
