// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::{
    tc::{
        TcAction, TcActionAttribute, TcActionGeneric, TcActionMirrorOption,
        TcActionOption, TcActionType, TcAttribute, TcFilterMatchAllOption,
        TcHandle, TcHeader, TcMessage, TcMessageBuffer, TcMirror,
        TcMirrorActionType, TcOption, TcStats2, TcStatsBasic, TcStatsQueue,
        Tcf,
    },
    AddressFamily,
};

// Setup:
//      ip link add dummy1 type dummy
//      ip link set dummy1 up
//      ip link add dummy2 type dummy
//      ip link set dummy2 up
//      tc qdisc  add dev dummy1 handle ffff: ingress
//      tc filter add dev dummy1 parent ffff: matchall \
//          action mirred egress mirror dev dummy2
//
// Capture nlmon of this command:
//
//      tc -s filter show dev dummy1
//
// Raw packet modification:
//   * rtnetlink header removed.
#[test]
fn test_get_filter_matchall() {
    let raw = vec![
        0x00, // AF_UNSPEC
        0x00, 0x00, 0x00, // padding
        0x32, 0x00, 0x00, 0x00, // iface index 50
        0x01, 0x00, 0x00, 0x00, // handle 0:1
        0x00, 0x00, 0x01, 0x00, // parent 1:0
        0x00, 0x03, 0x00, 0xc0, // info: 3221226240, TODO: no idea
        0x0d, 0x00, // length 13
        0x01, 0x00, // TCA_KIND
        0x6d, 0x61, 0x74, 0x63, 0x68, 0x61, 0x6c, 0x6c, 0x00, 0x00, 0x00, 0x00,
        // "matchall\0" and 3 bytes pad
        0x08, 0x00, // length 8
        0x0b, 0x00, // TCA_CHAIN
        0x00, 0x00, 0x00, 0x00, // chain: 0
        0xc0, 0x00, // length 192
        0x02, 0x00, // TCA_OPTIONS for `matchall`
        0x08, 0x00, // length 8
        0x03, 0x00, // TCA_MATCHALL_FLAGS
        0x08, 0x00, 0x00, 0x00, // flags: 8
        0x0c, 0x00, // length 12
        0x04, 0x00, // TCA_MATCHALL_PCNT
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TODO
        0xa8, 0x00, // length 168
        0x02, 0x00, // TCA_MATCHALL_ACT
        0xa4, 0x00, // length 164
        0x01, 0x00, // TCA_ACT_TAB
        0x0b, 0x00, // length 11
        0x01, 0x00, // TCA_ACT_KIND
        0x6d, 0x69, 0x72, 0x72, 0x65, 0x64, 0x00, 0x00,
        // "mirred\0" and 1 padding byte
        0x44, 0x00, // length 68
        0x04, 0x00, // TCA_ACT_STATS
        0x14, 0x00, // length 14
        0x01, 0x00, // TCA_STATS_BASIC
        0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // bytes: 70
        0x01, 0x00, 0x00, 0x00, // packet 1
        0x00, 0x00, 0x00, 0x00, // padding
        0x14, 0x00, // length 14
        0x07, 0x00, // TCA_STATS_BASIC_HW
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // bytes: 0
        0x00, 0x00, 0x00, 0x00, // packet 0
        0x00, 0x00, 0x00, 0x00, // padding
        0x18, 0x00, // length 24
        0x03, 0x00, // TCA_STATS_QUEUE
        0x00, 0x00, 0x00, 0x00, // qlen: 0
        0x00, 0x00, 0x00, 0x00, // backlog: 0
        0x00, 0x00, 0x00, 0x00, // drops: 0
        0x00, 0x00, 0x00, 0x00, // requeues: 0
        0x00, 0x00, 0x00, 0x00, // overlimits: 0
        0x08, 0x00, // length 8
        0x0a, 0x00, // TCA_ACT_IN_HW_COUNT
        0x00, 0x00, 0x00, 0x00, // 0
        0x48, 0x00, // length 72
        0x02, 0x80, // TCA_ACT_OPTIONS
        0x20, 0x00, // length 32
        0x02, 0x00, // TCA_MIRRED_PARMS
        0x01, 0x00, 0x00, 0x00, // index 1
        0x00, 0x00, 0x00, 0x00, // capab 0
        0x03, 0x00, 0x00, 0x00, // action 3
        0x01, 0x00, 0x00, 0x00, // refcount 1
        0x01, 0x00, 0x00, 0x00, // bindcnt 1
        0x02, 0x00, 0x00, 0x00, // eaction 2
        0x33, 0x00, 0x00, 0x00, // ifindex 51
        0x24, 0x00, // length 36
        0x01, 0x00, // TCA_MIRRED_TM
        0x90, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x02, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x02, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // TODO TCA_MIRRED_TM
    ];

    let expected = TcMessage {
        header: TcHeader {
            family: AddressFamily::Unspec,
            index: 50,
            handle: TcHandle { major: 0, minor: 1 },
            parent: TcHandle { major: 1, minor: 0 },
            info: 3221226240, // TODO(Gris Ge): What's this
        },
        attributes: vec![
            TcAttribute::Kind("matchall".to_string()),
            TcAttribute::Chain(0),
            TcAttribute::Options(vec![
                TcOption::MatchAll(TcFilterMatchAllOption::Flags(8)),
                TcOption::MatchAll(TcFilterMatchAllOption::Pnct(vec![
                    1, 0, 0, 0, 0, 0, 0, 0, // TODO(Gris Ge)
                ])),
                TcOption::MatchAll(TcFilterMatchAllOption::Action(vec![
                    TcAction {
                        tab: 1,
                        attributes: vec![
                            TcActionAttribute::Kind("mirred".to_string()),
                            TcActionAttribute::Stats(vec![
                                TcStats2::Basic(TcStatsBasic {
                                    bytes: 70,
                                    packets: 1,
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
                            TcActionAttribute::InHwCount(0),
                            TcActionAttribute::Options(vec![
                                TcActionOption::Mirror(
                                    TcActionMirrorOption::Parms(TcMirror {
                                        generic: TcActionGeneric {
                                            index: 1,
                                            capab: 0,
                                            action: TcActionType::Pipe,
                                            refcnt: 1,
                                            bindcnt: 1,
                                        },
                                        eaction:
                                            TcMirrorActionType::EgressMirror,
                                        ifindex: 51,
                                    }),
                                ),
                                TcActionOption::Mirror(
                                    TcActionMirrorOption::Tm(Tcf {
                                        install: 912,
                                        lastuse: 514,
                                        expires: 0,
                                        firstuse: 514,
                                    }),
                                ),
                            ]),
                        ],
                    },
                ])),
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
