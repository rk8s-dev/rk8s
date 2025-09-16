// SPDX-License-Identifier: MIT

use std::net::Ipv6Addr;
use std::str::FromStr;

use netlink_packet_utils::traits::{Emitable, Parseable};

use crate::route::flags::RouteFlags;
use crate::route::{
    RouteAttribute, RouteCacheInfo, RouteHeader, RouteMessage,
    RouteMessageBuffer, RouteMetric, RoutePreference, RouteProtocol,
    RouteScope, RouteType,
};
use crate::AddressFamily;

#[test]
// wireshark capture(netlink message header removed) of nlmon against command:
//   ip -6 route show dev wlan0
fn test_ipv6_route_cache_info() {
    let raw = vec![
        0x0a, // Address family inet6(10)
        0x00, // destination prefix length 0
        0x00, // source prefix length 0
        0x00, // tos
        0xfe, // table 254(main)
        0x09, // protocol RTPROT_RA 9
        0x00, // scope RT_SCOPE_UNIVERSE 0
        0x01, // type RTN_UNICAST 1
        0x00, 0x00, 0x00, 0x00, // flags 0
        0x08, 0x00, // length 8
        0x0f, 0x00, // RTA_TABLE 15
        0xfe, 0x00, 0x00, 0x00, // table u32 254
        0x14, 0x00, // length 20
        0x08, 0x00, // RTA_METRICS 8
        0x08, 0x00, // length 8
        0x02, 0x00, // RTAX_MTU 2
        0x98, 0x05, 0x00, 0x00, // MTU 1432
        0x08, 0x00, // length 8
        0x0a, 0x00, // RTAX_HOPLIMIT 10
        0xfe, 0x00, 0x00, 0x00, // hop limit 254
        0x08, 0x00, // length 8
        0x06, 0x00, // RTA_PRIORITY 6
        0x00, 0x04, 0x00, 0x00, // 1024
        0x14, 0x00, // length 20
        0x05, 0x00, // RTA_GATEWAY 5
        0xfe, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xd6, 0x38, 0x9c, 0xff,
        0xfe, 0x01, 0xe8, 0x52, // ipv6 addr
        0x08, 0x00, // length 8
        0x04, 0x00, // RTA_OIF 4
        0x02, 0x00, 0x00, 0x00, // oif 2
        0x24, 0x00, // length 36
        0x0c, 0x00, // RTA_CACHEINFO
        0x00, 0x00, 0x00, 0x00, // rta_clntref
        0x00, 0x00, 0x00, 0x00, // rta_lastuse
        0x29, 0x29, 0x05, 0x00, // rta_expires
        0x00, 0x00, 0x00, 0x00, // rta_error
        0x00, 0x00, 0x00, 0x00, // rta_id
        0x00, 0x00, 0x00, 0x00, // rta_ts
        0x00, 0x00, 0x00, 0x00, // rta_tsage
        0x00, 0x00, 0x00, 0x00, // padding
        0x05, 0x00, // length 5
        0x14, 0x00, // RTA_PREF
        0x01, // 1 ICMPV6_ROUTER_PREF_HIGH
        0x00, 0x00, 0x00, // padding
    ];

    let expected = RouteMessage {
        header: RouteHeader {
            address_family: AddressFamily::Inet6,
            destination_prefix_length: 0,
            source_prefix_length: 0,
            tos: 0,
            table: 254,
            protocol: RouteProtocol::Ra,
            scope: RouteScope::Universe,
            kind: RouteType::Unicast,
            flags: RouteFlags::empty(),
        },
        attributes: vec![
            RouteAttribute::Table(254),
            RouteAttribute::Metrics(vec![
                RouteMetric::Mtu(1432),
                RouteMetric::Hoplimit(254),
            ]),
            RouteAttribute::Priority(1024),
            RouteAttribute::Gateway(
                Ipv6Addr::from_str("fe80::d638:9cff:fe01:e852")
                    .unwrap()
                    .into(),
            ),
            RouteAttribute::Oif(2),
            RouteAttribute::CacheInfo(RouteCacheInfo {
                clntref: 0,
                last_use: 0,
                expires: 338217,
                error: 0,
                used: 0,
                id: 0,
                ts: 0,
                ts_age: 0,
            }),
            RouteAttribute::Preference(RoutePreference::High),
        ],
    };

    assert_eq!(
        expected,
        RouteMessage::parse(&RouteMessageBuffer::new(&raw)).unwrap()
    );

    let mut buf = vec![0; expected.buffer_len()];

    expected.emit(&mut buf);

    assert_eq!(buf, raw);
}
