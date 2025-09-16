// SPDX-License-Identifier: MIT

use netlink_packet_utils::nla::NlaBuffer;
use netlink_packet_utils::{Emitable, Parseable};

use crate::tc::{
    TcAction, TcActionAttribute, TcActionGeneric, TcActionGenericBuffer,
    TcActionMirrorOption, TcActionOption, TcActionType, TcMirror,
    TcMirrorActionType, TcMirrorBuffer, TcStats2, TcStatsBasic, TcStatsQueue,
    Tcf,
};

#[test]
fn tc_action_generic_parse_back() {
    let orig = TcActionGeneric {
        index: 1,
        capab: 2,
        action: TcActionType::Reclassify,
        refcnt: 3,
        bindcnt: 4,
    };
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcActionGeneric::parse(
        &TcActionGenericBuffer::new_checked(buffer).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}

#[test]
fn tc_mirror_default_parse_back() {
    let orig = TcMirror {
        generic: Default::default(),
        eaction: Default::default(),
        ifindex: 111,
    };
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcMirror::parse(
        &TcMirrorBuffer::new_checked(buffer.as_slice()).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}

#[test]
fn tc_mirror_example_parse_back() {
    let orig = TcMirror {
        generic: TcActionGeneric {
            index: 1,
            capab: 2,
            action: TcActionType::Ok,
            refcnt: 3,
            bindcnt: 4,
        },
        eaction: TcMirrorActionType::IngressMirror,
        ifindex: 99,
    };
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcMirror::parse(
        &TcMirrorBuffer::new_checked(buffer.as_slice()).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}

//      > act actions add action mirred egress redirect dev veth1
//      > tools/nl_dump.py dump_actions mirred
//      Note: 5.15 and 6.8 kernels do NOT set NLA_F_NESTED for TCA_ACT_OPTIONS
//
#[test]
fn get_mirred_eggress_redirect_action() {
    let raw = vec![
        0x9C, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x01, 0x00, 0x6D, 0x69, 0x72, 0x72,
        0x65, 0x64, 0x00, 0x00, 0x44, 0x00, 0x04, 0x00, 0x14, 0x00, 0x01, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x18, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x48, 0x00, 0x02, 0x80, 0x20, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
        0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
        0x24, 0x00, 0x01, 0x00, 0x68, 0x3D, 0xB6, 0x02, 0x00, 0x00, 0x00, 0x00,
        0x68, 0x3D, 0xB6, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let expected = TcAction {
        tab: 0,
        attributes: vec![
            TcActionAttribute::Kind("mirred".to_string()),
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
                TcActionOption::Mirror(TcActionMirrorOption::Parms(TcMirror {
                    generic: TcActionGeneric {
                        index: 1,
                        capab: 0,
                        action: TcActionType::Ok,
                        refcnt: 2,
                        bindcnt: 2,
                    },
                    eaction: TcMirrorActionType::EgressRedir,
                    ifindex: 3,
                })),
                TcActionOption::Mirror(TcActionMirrorOption::Tm(Tcf {
                    install: 45497704,
                    lastuse: 45497704,
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
