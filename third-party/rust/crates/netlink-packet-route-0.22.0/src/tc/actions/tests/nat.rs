// SPDX-License-Identifier: MIT

use std::net::Ipv4Addr;

use netlink_packet_utils::nla::NlaBuffer;
use netlink_packet_utils::{Emitable, Parseable};

use crate::tc::actions::message::TcActionMessage;
use crate::tc::actions::message::TcActionMessageAttribute::Actions;
use crate::tc::actions::{TcActionMessageBuffer, TcActionMessageHeader};
use crate::tc::TcActionAttribute::{InHwCount, Kind, Options, Stats};
use crate::tc::TcActionNatOption::{Parms, Tm};
use crate::tc::TcActionOption::Nat;
use crate::tc::TcStats2::{Basic, BasicHw, Queue};
use crate::tc::{
    TcAction, TcActionGeneric, TcActionNatOption, TcActionType, TcNat,
    TcNatFlags, TcStatsBasic, TcStatsQueue, Tcf,
};
use crate::AddressFamily;

/// Capture of request for
///
/// ```bash
/// tc actions add action nat ingress 1.2.3.4/32 5.6.7.0 index 1
/// ```
const TC_ACTION_NAT_EXAMPLE1: &[u8] = &[
    0x00, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x01, 0x00, 0x38, 0x00, 0x01, 0x00,
    0x08, 0x00, 0x01, 0x00, 0x6e, 0x61, 0x74, 0x00, 0x2c, 0x00, 0x02, 0x80,
    0x28, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x00, 0xff, 0xff, 0xff, 0xff,
    0x00, 0x00, 0x00, 0x00,
];

fn tc_action_message_nat_example1() -> TcActionMessage {
    TcActionMessage {
        header: TcActionMessageHeader {
            family: AddressFamily::Unspec,
        },
        attributes: vec![Actions(vec![TcAction {
            tab: 1,
            attributes: vec![
                Kind("nat".into()),
                Options(vec![Nat(Parms(TcNat {
                    generic: TcActionGeneric {
                        index: 1,
                        capab: 0,
                        action: TcActionType::Ok,
                        refcnt: 0,
                        bindcnt: 0,
                    },
                    old_addr: Ipv4Addr::new(1, 2, 3, 4),
                    new_addr: Ipv4Addr::new(5, 6, 7, 0),
                    mask: Ipv4Addr::new(255, 255, 255, 255),
                    flags: TcNatFlags::empty(),
                }))]),
            ],
        }])],
    }
}

#[test]
fn parse_tc_action_nat_example1() {
    let buf = TC_ACTION_NAT_EXAMPLE1;
    let parsed = TcActionMessage::parse(
        &TcActionMessageBuffer::new_checked(&buf).unwrap(),
    )
    .unwrap();
    assert_eq!(parsed, tc_action_message_nat_example1());
}

#[test]
fn emit_tc_action_nat_example1() {
    let example = tc_action_message_nat_example1();
    let mut buf = vec![0; example.buffer_len()];
    example.emit(&mut buf);
    assert_eq!(buf.as_slice(), TC_ACTION_NAT_EXAMPLE1);
}

/// Capture of request for
///
/// ```bash
/// tc actions add action nat ingress 1.2.3.0/24 5.6.7.9 index 2
/// ```
const TC_ACTION_NAT_EXAMPLE2: &[u8] = &[
    0x00, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x01, 0x00, 0x38, 0x00, 0x01, 0x00,
    0x08, 0x00, 0x01, 0x00, 0x6e, 0x61, 0x74, 0x00, 0x2c, 0x00, 0x02, 0x80,
    0x28, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x02, 0x03, 0x00, 0x05, 0x06, 0x07, 0x09, 0xff, 0xff, 0xff, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

fn tc_action_message_nat_example2() -> TcActionMessage {
    TcActionMessage {
        header: TcActionMessageHeader {
            family: AddressFamily::Unspec,
        },
        attributes: vec![Actions(vec![TcAction {
            tab: 1,
            attributes: vec![
                Kind("nat".into()),
                Options(vec![Nat(Parms(TcNat {
                    generic: TcActionGeneric {
                        index: 2,
                        capab: 0,
                        action: TcActionType::Ok,
                        refcnt: 0,
                        bindcnt: 0,
                    },
                    old_addr: Ipv4Addr::new(1, 2, 3, 0),
                    new_addr: Ipv4Addr::new(5, 6, 7, 9),
                    mask: Ipv4Addr::new(255, 255, 255, 0),
                    flags: TcNatFlags::empty(),
                }))]),
            ],
        }])],
    }
}

#[test]
fn parse_tc_action_nat_example2() {
    let buf = TC_ACTION_NAT_EXAMPLE2;
    let parsed = TcActionMessage::parse(
        &TcActionMessageBuffer::new_checked(&buf).unwrap(),
    )
    .unwrap();
    assert_eq!(parsed, tc_action_message_nat_example2());
}

#[test]
fn emit_tc_action_nat_example2() {
    let example = tc_action_message_nat_example2();
    let mut buf = vec![0; example.buffer_len()];
    example.emit(&mut buf);
    assert_eq!(buf.as_slice(), TC_ACTION_NAT_EXAMPLE2);
}

/// Capture of request for
///
/// ```bash
/// tc actions add action nat egress 2.3.4.0/24 5.6.7.9 index 3
/// ```
const TC_ACTION_NAT_EXAMPLE3: &[u8] = &[
    0x00, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x01, 0x00, 0x38, 0x00, 0x01, 0x00,
    0x08, 0x00, 0x01, 0x00, 0x6e, 0x61, 0x74, 0x00, 0x2c, 0x00, 0x02, 0x80,
    0x28, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x02, 0x03, 0x04, 0x00, 0x05, 0x06, 0x07, 0x09, 0xff, 0xff, 0xff, 0x00,
    0x01, 0x00, 0x00, 0x00,
];

fn tc_action_message_nat_example3() -> TcActionMessage {
    TcActionMessage {
        header: TcActionMessageHeader {
            family: AddressFamily::Unspec,
        },
        attributes: vec![Actions(vec![TcAction {
            tab: 1,
            attributes: vec![
                Kind("nat".into()),
                Options(vec![Nat(Parms(TcNat {
                    generic: TcActionGeneric {
                        index: 3,
                        capab: 0,
                        action: TcActionType::Ok,
                        refcnt: 0,
                        bindcnt: 0,
                    },
                    old_addr: Ipv4Addr::new(2, 3, 4, 0),
                    new_addr: Ipv4Addr::new(5, 6, 7, 9),
                    mask: Ipv4Addr::new(255, 255, 255, 0),
                    flags: TcNatFlags::Egress,
                }))]),
            ],
        }])],
    }
}

#[test]
fn parse_tc_action_nat_example3() {
    let buf = TC_ACTION_NAT_EXAMPLE3;
    let parsed = TcActionMessage::parse(
        &TcActionMessageBuffer::new_checked(&buf).unwrap(),
    )
    .unwrap();
    assert_eq!(parsed, tc_action_message_nat_example3());
}

#[test]
fn emit_tc_action_nat_example3() {
    let example = tc_action_message_nat_example3();
    let mut buf = vec![0x00; example.buffer_len()];
    example.emit(&mut buf);
    assert_eq!(buf.as_slice(), TC_ACTION_NAT_EXAMPLE3);
}

const TC_ACTION_NAT_OPTION_PARAMS_EXAMPLES: [TcActionNatOption; 2] = [
    Parms(TcNat {
        flags: TcNatFlags::empty(),
        generic: TcActionGeneric {
            action: TcActionType::Reclassify,
            bindcnt: 1,
            capab: 2,
            index: 3,
            refcnt: 4,
        },
        mask: Ipv4Addr::BROADCAST,
        new_addr: Ipv4Addr::new(1, 2, 3, 4),
        old_addr: Ipv4Addr::new(5, 6, 7, 8),
    }),
    Parms(TcNat {
        flags: TcNatFlags::empty(),
        generic: TcActionGeneric {
            action: TcActionType::Pipe,
            bindcnt: 5,
            capab: 6,
            index: 7,
            refcnt: 8,
        },
        mask: Ipv4Addr::new(255, 255, 255, 254),
        new_addr: Ipv4Addr::new(2, 1, 255, 0),
        old_addr: Ipv4Addr::new(7, 2, 88, 44),
    }),
];

#[test]
fn tc_action_nat_option_parse_back_example_params() {
    for example in TC_ACTION_NAT_OPTION_PARAMS_EXAMPLES {
        let mut buffer = vec![0; example.buffer_len()];
        example.emit(&mut buffer);
        let parsed = TcActionNatOption::parse(
            &NlaBuffer::new_checked(buffer.as_slice()).unwrap(),
        )
        .unwrap();
        assert_eq!(example, parsed);
    }
}

#[test]
fn tc_action_nat_option_emit_uses_whole_buffer() {
    for example in TC_ACTION_NAT_OPTION_PARAMS_EXAMPLES {
        let mut buffer1 = vec![0x00; example.buffer_len()];
        let mut buffer2 = vec![0xff; example.buffer_len()];
        example.emit(&mut buffer1);
        example.emit(&mut buffer2);
        assert_eq!(buffer1, buffer2);
    }
}

/// Setup:
///
/// ```bash
/// tc actions flush action nat
/// tc actions add action nat ingress 192.0.2.1/32 203.0.113.1 index 1
/// ```
///
/// Then capture netlink response message of this command:
///
/// ```bash
/// tc -statistics actions get action nat index 1
/// ```
///
/// Raw packet modification:
///   * cooked header removed (16 bytes).
///   * rtnetlink header removed (16 bytes).
#[test]
fn test_get_filter_nat() {
    const RAW: &[u8] = &[
        0x00, 0x00, 0x00, 0x00, 0xac, 0x00, 0x01, 0x00, 0xa8, 0x00, 0x01, 0x00,
        0x08, 0x00, 0x01, 0x00, 0x6e, 0x61, 0x74, 0x00, 0x44, 0x00, 0x04, 0x00,
        0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x07, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x18, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x50, 0x00, 0x02, 0x80, 0x28, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0xc0, 0x00, 0x02, 0x01, 0xcb, 0x00, 0x71, 0x01,
        0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x24, 0x00, 0x02, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let expected = TcActionMessage {
        header: TcActionMessageHeader {
            family: AddressFamily::Unspec,
        },
        attributes: vec![Actions(vec![TcAction {
            tab: 1,
            attributes: vec![
                Kind("nat".into()),
                Stats(vec![
                    Basic(TcStatsBasic {
                        bytes: 0,
                        packets: 0,
                    }),
                    BasicHw(TcStatsBasic {
                        bytes: 0,
                        packets: 0,
                    }),
                    Queue(TcStatsQueue {
                        qlen: 0,
                        backlog: 0,
                        drops: 0,
                        requeues: 0,
                        overlimits: 0,
                    }),
                ]),
                InHwCount(0),
                Options(vec![
                    Nat(Parms(TcNat {
                        generic: TcActionGeneric {
                            index: 1,
                            capab: 0,
                            action: TcActionType::Ok,
                            refcnt: 1,
                            bindcnt: 0,
                        },
                        old_addr: [192, 0, 2, 1].into(),
                        new_addr: [203, 0, 113, 1].into(),
                        mask: Ipv4Addr::BROADCAST,
                        flags: TcNatFlags::empty(),
                    })),
                    Nat(Tm(Tcf {
                        install: 0,
                        lastuse: 0,
                        expires: 0,
                        firstuse: 0,
                    })),
                ]),
            ],
        }])],
    };

    assert_eq!(
        expected,
        TcActionMessage::parse(&TcActionMessageBuffer::new(&RAW)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, RAW);
}
