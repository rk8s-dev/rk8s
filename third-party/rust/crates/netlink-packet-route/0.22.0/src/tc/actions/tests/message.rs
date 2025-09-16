// SPDX-License-Identifier: MIT

use netlink_packet_utils::nla::{DefaultNla, NlaBuffer};
use netlink_packet_utils::{Emitable, Parseable};

use crate::tc::actions::message::TcActionMessageAttribute::{
    Actions, Flags, RootCount, RootExtWarnMsg, RootTimeDelta,
};
use crate::tc::actions::message::{
    TcActionMessage, TcActionMessageAttribute, TcActionMessageFlags,
    TcActionMessageFlagsWithSelector,
};
use crate::tc::actions::{TcActionMessageBuffer, TcActionMessageHeader};
use crate::tc::TcAction;
use crate::tc::TcActionAttribute::{Cookie, Index, Kind};
use crate::AddressFamily;

mod mirror {
    use netlink_packet_utils::nla::DefaultNla;
    use netlink_packet_utils::Parseable;

    use crate::tc::actions::message::TcActionMessage;
    use crate::tc::actions::message::TcActionMessageAttribute::{
        Actions, RootCount,
    };
    use crate::tc::actions::{TcActionMessageBuffer, TcActionMessageHeader};
    use crate::tc::TcActionAttribute::{
        InHwCount, Kind, Options, Other, Stats,
    };
    use crate::tc::TcActionMirrorOption::{Parms, Tm};
    use crate::tc::TcActionOption::Mirror;
    use crate::tc::TcActionType::{Pipe, Stolen};
    use crate::tc::TcMirrorActionType::{EgressRedir, IngressMirror};
    use crate::tc::TcStats2::{Basic, BasicHw, Queue};
    use crate::tc::{
        TcAction, TcActionGeneric, TcMirror, TcStatsBasic, TcStatsQueue, Tcf,
    };
    use crate::AddressFamily;

    /// Captured `TcActionMessage` examples used for testing.
    mod message {
        /// Capture of request message for
        ///
        /// ```bash
        /// tc actions add action mirred egress redirect dev lo index 1
        /// ```
        pub(super) const CREATE1: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, 0x38, 0x00, 0x01, 0x00, 0x34, 0x00, 0x01,
            0x00, 0x0b, 0x00, 0x01, 0x00, 0x6d, 0x69, 0x72, 0x72, 0x65, 0x64,
            0x00, 0x00, 0x24, 0x00, 0x02, 0x80, 0x20, 0x00, 0x02, 0x00, 0x01,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x00, 0x00,
        ];
        /// Capture of request message for
        ///
        /// ```bash
        /// tc actions add action mirred ingress mirror dev lo index 2
        /// ```
        pub(super) const CREATE2: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, 0x38, 0x00, 0x01, 0x00, 0x34, 0x00, 0x01,
            0x00, 0x0b, 0x00, 0x01, 0x00, 0x6d, 0x69, 0x72, 0x72, 0x65, 0x64,
            0x00, 0x00, 0x24, 0x00, 0x02, 0x80, 0x20, 0x00, 0x02, 0x00, 0x02,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x00, 0x00,
        ];
        /// Capture of request message for
        ///
        /// ```bash
        /// tc actions list action mirred
        /// ```
        ///
        /// after the messages in [`CREATE1`] and [`CREATE2`] have been added.
        pub(super) const LIST: &[u8] = &[
            0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x03, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x64, 0x01, 0x01, 0x00, 0xb0, 0x00, 0x00, 0x00, 0x0b, 0x00,
            0x01, 0x00, 0x6d, 0x69, 0x72, 0x72, 0x65, 0x64, 0x00, 0x00, 0x44,
            0x00, 0x04, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x14, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18,
            0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x0c, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x08, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48,
            0x00, 0x02, 0x00, 0x20, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x00, 0x00, 0x24, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0xb0, 0x00, 0x01, 0x00, 0x0b, 0x00,
            0x01, 0x00, 0x6d, 0x69, 0x72, 0x72, 0x65, 0x64, 0x00, 0x00, 0x44,
            0x00, 0x04, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x14, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18,
            0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x0c, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00,
            0x00, 0x00, 0x08, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48,
            0x00, 0x02, 0x00, 0x20, 0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x00, 0x00, 0x24, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00,
        ];
    }

    #[test]
    fn parse_message1_create() {
        let expected = TcActionMessage {
            header: TcActionMessageHeader {
                family: AddressFamily::Unspec,
            },
            attributes: vec![Actions(vec![TcAction {
                tab: 1,
                attributes: vec![
                    Kind("mirred".into()),
                    Options(vec![Mirror(Parms(TcMirror {
                        generic: TcActionGeneric {
                            index: 1,
                            capab: 0,
                            action: Stolen,
                            refcnt: 0,
                            bindcnt: 0,
                        },
                        eaction: EgressRedir,
                        ifindex: 1,
                    }))]),
                ],
            }])],
        };

        let parsed = TcActionMessage::parse(
            &TcActionMessageBuffer::new_checked(&message::CREATE1).unwrap(),
        )
        .unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_message2_create() {
        let expected = TcActionMessage {
            header: TcActionMessageHeader {
                family: AddressFamily::Unspec,
            },
            attributes: vec![Actions(vec![TcAction {
                tab: 1,
                attributes: vec![
                    Kind("mirred".into()),
                    Options(vec![Mirror(Parms(TcMirror {
                        generic: TcActionGeneric {
                            index: 2,
                            capab: 0,
                            action: Pipe,
                            refcnt: 0,
                            bindcnt: 0,
                        },
                        eaction: IngressMirror,
                        ifindex: 1,
                    }))]),
                ],
            }])],
        };

        let buf = message::CREATE2;
        let parsed = TcActionMessage::parse(
            &TcActionMessageBuffer::new_checked(&buf).unwrap(),
        )
        .unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn parse_message3_list() {
        let expected = TcActionMessage {
            header: TcActionMessageHeader {
                family: AddressFamily::Unspec,
            },
            attributes: vec![
                RootCount(2),
                Actions(vec![
                    TcAction {
                        tab: 0,
                        attributes: vec![
                            Kind("mirred".into()),
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
                            Other(DefaultNla::new(
                                9,
                                vec![0, 0, 0, 0, 3, 0, 0, 0],
                            )),
                            InHwCount(0),
                            Options(vec![
                                Mirror(Parms(TcMirror {
                                    generic: TcActionGeneric {
                                        index: 1,
                                        capab: 0,
                                        action: Stolen,
                                        refcnt: 1,
                                        bindcnt: 0,
                                    },
                                    eaction: EgressRedir,
                                    ifindex: 1,
                                })),
                                Mirror(Tm(Tcf {
                                    install: 0,
                                    lastuse: 0,
                                    expires: 0,
                                    firstuse: 0,
                                })),
                            ]),
                        ],
                    },
                    TcAction {
                        tab: 1,
                        attributes: vec![
                            Kind("mirred".into()),
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
                            Other(DefaultNla::new(
                                9,
                                vec![0, 0, 0, 0, 3, 0, 0, 0],
                            )),
                            InHwCount(0),
                            Options(vec![
                                Mirror(Parms(TcMirror {
                                    generic: TcActionGeneric {
                                        index: 2,
                                        capab: 0,
                                        action: Pipe,
                                        refcnt: 1,
                                        bindcnt: 0,
                                    },
                                    eaction: IngressMirror,
                                    ifindex: 1,
                                })),
                                Mirror(Tm(Tcf {
                                    install: 0,
                                    lastuse: 0,
                                    expires: 0,
                                    firstuse: 0,
                                })),
                            ]),
                        ],
                    },
                ]),
            ],
        };
        let parsed = TcActionMessage::parse(
            &TcActionMessageBuffer::new_checked(&message::LIST).unwrap(),
        )
        .unwrap();
        assert_eq!(parsed, expected);
    }
}

#[test]
fn tc_action_message_attribute_parse_back_blank_actions() {
    let orig = Actions(vec![]);
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcActionMessageAttribute::parse(
        &NlaBuffer::new_checked(buffer.as_slice()).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}

#[test]
fn tc_action_message_attribute_parse_back_example_action() {
    let orig = Actions(vec![TcAction {
        tab: 9999,
        attributes: vec![Kind("example".into())],
    }]);
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcActionMessageAttribute::parse(
        &NlaBuffer::new_checked(buffer.as_slice()).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}

#[test]
fn tc_action_message_attribute_parse_back_multiple_example_action() {
    let orig = Actions(vec![
        TcAction {
            tab: 1111,
            attributes: vec![Kind("example1".into())],
        },
        TcAction {
            tab: 2222,
            attributes: vec![
                Kind("example2".into()),
                Index(42),
                Cookie(vec![1, 2, 3, 4, 5, 6]),
            ],
        },
    ]);
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcActionMessageAttribute::parse(
        &NlaBuffer::new_checked(buffer.as_slice()).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}

#[test]
fn tc_action_message_flags_parse_back_default() {
    let orig = TcActionMessageFlagsWithSelector::default();
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcActionMessageFlagsWithSelector::parse(
        &NlaBuffer::new_checked(buffer.as_slice()).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}

#[test]
fn tc_action_message_flags_parse_back_example_value() {
    let orig = TcActionMessageFlagsWithSelector {
        flags: TcActionMessageFlags::LargeDump
            | TcActionMessageFlags::TerseDump,
        selector: TcActionMessageFlags::LargeDump,
    };
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcActionMessageFlagsWithSelector::parse(
        &NlaBuffer::new_checked(buffer.as_slice()).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}

#[test]
fn tc_action_message_parse_back_default() {
    let orig = TcActionMessage::default();
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcActionMessage::parse(
        &TcActionMessageBuffer::new_checked(buffer.as_slice()).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}

#[test]
fn tc_action_message_parse_back_example_value() {
    let orig = TcActionMessage {
        header: TcActionMessageHeader {
            family: AddressFamily::Alg,
        },
        attributes: vec![
            Flags(TcActionMessageFlagsWithSelector {
                flags: TcActionMessageFlags::LargeDump,
                selector: TcActionMessageFlags::LargeDump,
            }),
            RootCount(42),
            RootTimeDelta(43),
            RootExtWarnMsg("hello".to_string()),
            TcActionMessageAttribute::Other(DefaultNla::new(
                99,
                vec![1, 2, 3, 4],
            )),
        ],
    };
    let mut buffer = vec![0; orig.buffer_len()];
    orig.emit(&mut buffer);
    let parsed = TcActionMessage::parse(
        &TcActionMessageBuffer::new_checked(buffer.as_slice()).unwrap(),
    )
    .unwrap();
    assert_eq!(orig, parsed);
}
