// SPDX-License-Identifier: MIT

use std::{net::Ipv6Addr, str::FromStr};

use netlink_packet_utils::{Emitable, Parseable};

use crate::prefix::{
    attribute::PrefixAttribute, cache_info::CacheInfo, header::PrefixHeader,
    PrefixMessage, PrefixMessageBuffer,
};

#[test]
fn test_new_prefix() {
    #[rustfmt::skip]
    let data = vec![
        // prefixmsg
        // AF_INET6 + padding
        0x0a, 0x00, 0x00, 0x00,
        // ifindex
        0x02, 0x00, 0x00, 0x00,
        // type, prefix length, flags, padding
        0x03, 0x40, 0x01, 0x00,
        // PREFIX_ADDRESS attribute
        0x14, 0x00, 0x01, 0x00,
        0xfc, 0x00, 0x0a, 0x0a, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        // PREFIX_CACHEINFO attribute
        0x0c, 0x00, 0x02, 0x00,
        0xff, 0xff, 0xff, 0xfa,
        0xff, 0xff, 0xff, 0xff,
    ];
    let actual = PrefixMessage::parse(&PrefixMessageBuffer::new(&data))
        .expect("Generated PrefixMessage");

    let expected = PrefixMessage {
        header: PrefixHeader {
            prefix_family: libc::AF_INET6 as u8,
            ifindex: 2,
            prefix_type: 3,
            prefix_len: 64,
            flags: 1,
        },
        attributes: vec![
            PrefixAttribute::Address(
                Ipv6Addr::from_str("fc00:a0a::1").expect("Ipv6Addr"),
            ),
            PrefixAttribute::CacheInfo(CacheInfo {
                preferred_time: 0xfaffffff,
                valid_time: 0xffffffff,
            }),
        ],
    };

    assert_eq!(expected, actual);

    let mut buf = vec![0; 44];
    expected.emit(&mut buf);
    assert_eq!(data, buf);
}
