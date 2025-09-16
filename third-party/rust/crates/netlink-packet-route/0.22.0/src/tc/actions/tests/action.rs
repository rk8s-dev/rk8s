// SPDX-License-Identifier: MIT

use crate::tc::{
    TcAction, TcActionAttribute, TcActionGeneric, TcActionGenericBuffer,
    TcActionType, TcStats2, TcStatsBasic,
};
use netlink_packet_utils::nla::NlaBuffer;
use netlink_packet_utils::{Emitable, Parseable};

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
fn tc_action_parse_back_minimal() {
    let orig = TcAction {
        tab: 1,
        attributes: vec![TcActionAttribute::Kind("example".into())],
    };
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed =
        TcAction::parse(&NlaBuffer::new_checked(buffer.as_slice()).unwrap())
            .unwrap();
    assert_eq!(orig, parsed);
}

#[test]
fn tc_action_parse_back_example() {
    let orig = TcAction {
        tab: 1,
        attributes: vec![
            TcActionAttribute::Kind("example".into()),
            TcActionAttribute::Index(1),
            TcActionAttribute::Cookie(vec![1, 2, 3, 4, 5, 6, 7, 8]),
            TcActionAttribute::InHwCount(99),
            TcActionAttribute::Stats(vec![
                TcStats2::Basic(TcStatsBasic {
                    bytes: 1,
                    packets: 2,
                }),
                TcStats2::BasicHw(TcStatsBasic {
                    bytes: 3,
                    packets: 4,
                }),
            ]),
            TcActionAttribute::Options(vec![]),
        ],
    };
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed =
        TcAction::parse(&NlaBuffer::new_checked(buffer.as_slice()).unwrap())
            .unwrap();
    assert_eq!(orig, parsed);
}
