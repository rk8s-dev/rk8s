// SPDX-License-Identifier: MIT

use netlink_packet_utils::{Emitable, Parseable};

use crate::{
    tc::{
        TcAttribute, TcFqCodelQdStats, TcFqCodelXstats, TcHandle, TcHeader,
        TcMessage, TcMessageBuffer, TcOption, TcQdiscFqCodelOption, TcStats,
        TcStats2, TcStatsBasic, TcStatsQueue, TcXstats,
    },
    AddressFamily,
};

// Setup:
//  * Connect a OpenVPN
//
// Capture nlmon of this command:
//
//      tc -s qdisc show dev openvpn0
//
// Raw packet modification:
//   * rtnetlink header removed.
#[test]
fn test_get_qdisc_fq_codel() {
    let raw = vec![
        0x00, // AF_UNSPEC
        0x00, 0x00, 0x00, // padding
        0x1c, 0x00, 0x00, 0x00, // iface index: 28
        0x00, 0x00, 0x00, 0x00, // handle 0:0 (TC_H_UNSPEC)
        0xff, 0xff, 0xff, 0xff, // parent u32::MAX (TC_H_ROOT)
        0x02, 0x00, 0x00, 0x00, // info(refcount): 2
        0x0d, 0x00, // length 13
        0x01, 0x00, // TCA_KIND
        0x66, 0x71, 0x5f, 0x63, 0x6f, 0x64, 0x65, 0x6c, 0x00, 0x00, 0x00, 0x00,
        // "fq_codel\0" and 3 bytes pad
        0x44, 0x00, // length 68
        0x02, 0x00, // TCA_OPTIONS for `fq_codel`
        0x08, 0x00, // length 8
        0x01, 0x00, // TCA_FQ_CODEL_TARGET
        0x87, 0x13, 0x00, 0x00, // 4999
        0x08, 0x00, // length 8
        0x02, 0x00, // TCA_FQ_CODEL_LIMIT
        0x00, 0x28, 0x00, 0x00, // 10240
        0x08, 0x00, // length 8
        0x03, 0x00, // TCA_FQ_CODEL_INTERVAL
        0x9f, 0x86, 0x01, 0x00, // 99999
        0x08, 0x00, // length 8
        0x04, 0x00, // TCA_FQ_CODEL_ECN
        0x01, 0x00, 0x00, 0x00, // 1
        0x08, 0x00, // length 8
        0x06, 0x00, // TCA_FQ_CODEL_QUANTUM
        0x3c, 0x05, 0x00, 0x00, // 1340
        0x08, 0x00, // length 8
        0x08, 0x00, // TCA_FQ_CODEL_DROP_BATCH_SIZE
        0x40, 0x00, 0x00, 0x00, // 64
        0x08, 0x00, // length 8
        0x09, 0x00, // TCA_FQ_CODEL_MEMORY_LIMIT
        0x00, 0x00, 0x00, 0x02, // 33554432
        0x08, 0x00, // length 8
        0x05, 0x00, // TCA_FQ_CODEL_FLOWS
        0x00, 0x04, 0x00, 0x00, // 1024
        0x05, 0x00, // length 5
        0x0c, 0x00, // TCA_HW_OFFLOAD
        0x00, 0x00, 0x00, 0x00, // 0 with padding
        0x5c, 0x00, // length 92
        0x07, 0x00, // TCA_STATS2
        0x2c, 0x00, // length 44
        0x04, 0x00, // TCA_STATS_APP
        0x00, 0x00, 0x00, 0x00, // TCA_FQ_CODEL_XSTATS_QDISC
        0x70, 0x01, 0x00, 0x00, // maxpacket: 368
        0x00, 0x00, 0x00, 0x00, // drop_overlimit: 0
        0x00, 0x00, 0x00, 0x00, // ecn_mark: 0
        0x24, 0x00, 0x00, 0x00, // new_flow_count: 36
        0x00, 0x00, 0x00, 0x00, // new_flows_len: 0
        0x00, 0x00, 0x00, 0x00, // old_flows_len: 0
        0x00, 0x00, 0x00, 0x00, // ce_mark: 0
        0x00, 0x00, 0x00, 0x00, // memory_usage: 0
        0x00, 0x00, 0x00, 0x00, // drop_overmemory: 0
        0x14, 0x00, // length 20
        0x01, 0x00, // TCA_STATS_BASIC
        0xe2, 0x47, 0xe0, 0x00, 0x00, 0x00, 0x00, 0x00, // bytes: 14698466
        0xb9, 0x67, 0x01, 0x00, // packets: 92089
        0x00, 0x00, 0x00, 0x00, // padding
        0x18, 0x00, // length 24
        0x03, 0x00, // TCA_STATS_QUEUE
        0x00, 0x00, 0x00, 0x00, // qlen: 0
        0x00, 0x00, 0x00, 0x00, // backlog: 0
        0x00, 0x00, 0x00, 0x00, // drops: 0
        0x00, 0x00, 0x00, 0x00, // requeues: 0
        0x00, 0x00, 0x00, 0x00, // overlimits: 0
        0x2c, 0x00, // length 44
        0x03, 0x00, // TCA_STATS
        0xe2, 0x47, 0xe0, 0x00, 0x00, 0x00, 0x00, 0x00, // bytes: 14698466
        0xb9, 0x67, 0x01, 0x00, // packets: 92089
        0x00, 0x00, 0x00, 0x00, // drops: 0
        0x00, 0x00, 0x00, 0x00, // overlimits: 0
        0x00, 0x00, 0x00, 0x00, // bps: 0
        0x00, 0x00, 0x00, 0x00, // pps: 0
        0x00, 0x00, 0x00, 0x00, // qlen: 0
        0x00, 0x00, 0x00, 0x00, // backlog: 0
        0x00, 0x00, 0x00, 0x00, // padding
        0x2c, 0x00, // length 44
        0x04, 0x00, // TCA_XSTATS
        0x00, 0x00, 0x00, 0x00, // TCA_FQ_CODEL_XSTATS_QDISC
        0x70, 0x01, 0x00, 0x00, // maxpacket: 368
        0x00, 0x00, 0x00, 0x00, // drop_overlimit: 0
        0x00, 0x00, 0x00, 0x00, // ecn_mark: 0
        0x24, 0x00, 0x00, 0x00, // new_flow_count: 36
        0x00, 0x00, 0x00, 0x00, // new_flows_len: 0
        0x00, 0x00, 0x00, 0x00, // old_flows_len: 0
        0x00, 0x00, 0x00, 0x00, // ce_mark: 0
        0x00, 0x00, 0x00, 0x00, // memory_usage: 0
        0x00, 0x00, 0x00, 0x00, // drop_overmemory: 0
    ];

    let expected = TcMessage {
        header: TcHeader {
            family: AddressFamily::Unspec,
            index: 28,
            handle: TcHandle::UNSPEC,
            parent: TcHandle::ROOT,
            info: 2,
        },
        attributes: vec![
            TcAttribute::Kind("fq_codel".to_string()),
            TcAttribute::Options(vec![
                TcOption::FqCodel(TcQdiscFqCodelOption::Target(4999)),
                TcOption::FqCodel(TcQdiscFqCodelOption::Limit(10240)),
                TcOption::FqCodel(TcQdiscFqCodelOption::Interval(99999)),
                TcOption::FqCodel(TcQdiscFqCodelOption::Ecn(1)),
                TcOption::FqCodel(TcQdiscFqCodelOption::Quantum(1340)),
                TcOption::FqCodel(TcQdiscFqCodelOption::DropBatchSize(64)),
                TcOption::FqCodel(TcQdiscFqCodelOption::MemoryLimit(33554432)),
                TcOption::FqCodel(TcQdiscFqCodelOption::Flows(1024)),
            ]),
            TcAttribute::HwOffload(0),
            TcAttribute::Stats2(vec![
                TcStats2::App(TcXstats::FqCodel(TcFqCodelXstats::Qdisc(
                    TcFqCodelQdStats {
                        maxpacket: 368,
                        drop_overlimit: 0,
                        ecn_mark: 0,
                        new_flow_count: 36,
                        new_flows_len: 0,
                        old_flows_len: 0,
                        ce_mark: 0,
                        memory_usage: 0,
                        drop_overmemory: 0,
                    },
                ))),
                TcStats2::Basic(TcStatsBasic {
                    bytes: 14698466,
                    packets: 92089,
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
                bytes: 14698466,
                packets: 92089,
                drops: 0,
                overlimits: 0,
                bps: 0,
                pps: 0,
                qlen: 0,
                backlog: 0,
            }),
            TcAttribute::Xstats(TcXstats::FqCodel(TcFqCodelXstats::Qdisc(
                TcFqCodelQdStats {
                    maxpacket: 368,
                    drop_overlimit: 0,
                    ecn_mark: 0,
                    new_flow_count: 36,
                    new_flows_len: 0,
                    old_flows_len: 0,
                    ce_mark: 0,
                    memory_usage: 0,
                    drop_overmemory: 0,
                },
            ))),
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
