// SPDX-License-Identifier: MIT

use netlink_packet_utils::{
    nla::{DefaultNla, NlaBuffer},
    Emitable, Parseable,
};

use crate::link::{xdp::VecLinkXdp, LinkXdp, XdpAttached};

static ATTACHED: [u8; 48] = [
    0x05, 0x00, // length = 5
    0x02, 0x00, // type = 2 = IFLA_XDP_ATTACHED
    0x00, 0x00, // none = XDP_ATTACHED_NONE
    0x00, 0x00, // padding
    0x05, 0x00, // length = 5
    0x02, 0x00, // type = 2 = IFLA_XDP_ATTACHED
    0x01, 0x00, // driver = XDP_ATTACHED_DRV
    0x00, 0x00, // padding
    0x05, 0x00, // length = 5
    0x02, 0x00, // type = 2 = IFLA_XDP_ATTACHED
    0x02, 0x00, // skb = XDP_ATTACHED_SKB
    0x00, 0x00, // padding
    0x05, 0x00, // length = 5
    0x02, 0x00, // type = 2 = IFLA_XDP_ATTACHED
    0x03, 0x00, // hw = XDP_ATTACHED_HW
    0x00, 0x00, // padding
    0x05, 0x00, // length = 5
    0x02, 0x00, // type = 2 = IFLA_XDP_ATTACHED
    0x04, 0x00, // multi = XDP_ATTACHED_MULTI
    0x00, 0x00, // padding
    0x05, 0x00, // length = 5
    0x02, 0x00, // type = 2 = IFLA_XDP_ATTACHED
    0xfc, 0x00, // other = random number = 252
    0x00, 0x00, // padding
];

#[test]
fn parse_xdp_attached() {
    let nla = NlaBuffer::new_checked(&ATTACHED[..]).unwrap();
    let parsed = VecLinkXdp::parse(&nla).unwrap().0;
    let expected = vec![
        LinkXdp::Attached(XdpAttached::None),
        LinkXdp::Attached(XdpAttached::Driver),
        LinkXdp::Attached(XdpAttached::SocketBuffer),
        LinkXdp::Attached(XdpAttached::Hardware),
        LinkXdp::Attached(XdpAttached::Multiple),
        LinkXdp::Attached(XdpAttached::Other(252)),
    ];
    assert_eq!(expected, parsed);
}

#[test]
fn emit_xdp_attached() {
    // None
    let nlas = vec![LinkXdp::Attached(XdpAttached::None)];
    assert_eq!(nlas.as_slice().buffer_len(), 8);

    let mut vec = vec![0xff; 8];
    nlas.as_slice().emit(&mut vec);
    assert_eq!(&vec[..], &ATTACHED[..8]);

    // Driver
    let nlas = vec![LinkXdp::Attached(XdpAttached::Driver)];
    assert_eq!(nlas.as_slice().buffer_len(), 8);

    let mut vec = vec![0xff; 8];
    nlas.as_slice().emit(&mut vec);
    assert_eq!(&vec[..], &ATTACHED[8..16]);

    // SocketBuffer/skb
    let nlas = vec![LinkXdp::Attached(XdpAttached::SocketBuffer)];
    assert_eq!(nlas.as_slice().buffer_len(), 8);

    let mut vec = vec![0xff; 8];
    nlas.as_slice().emit(&mut vec);
    assert_eq!(&vec[..], &ATTACHED[16..24]);

    // Hardware
    let nlas = vec![LinkXdp::Attached(XdpAttached::Hardware)];
    assert_eq!(nlas.as_slice().buffer_len(), 8);

    let mut vec = vec![0xff; 8];
    nlas.as_slice().emit(&mut vec);
    assert_eq!(&vec[..], &ATTACHED[24..32]);

    // Multiple
    let nlas = vec![LinkXdp::Attached(XdpAttached::Multiple)];
    assert_eq!(nlas.as_slice().buffer_len(), 8);

    let mut vec = vec![0xff; 8];
    nlas.as_slice().emit(&mut vec);
    assert_eq!(&vec[..], &ATTACHED[32..40]);

    // Multiple
    let nlas = vec![LinkXdp::Attached(XdpAttached::Other(252))];
    assert_eq!(nlas.as_slice().buffer_len(), 8);

    let mut vec = vec![0xff; 8];
    nlas.as_slice().emit(&mut vec);
    assert_eq!(&vec[..], &ATTACHED[40..48]);
}

#[rustfmt::skip]
    static XDP: [u8; 72] = [
        0x08, 0x00, // length = 8
        0x01, 0x00, // type = 1 = IFLA_XDP_FD
        0xA0, 0x74, 0x00, 0x00, // 29856
        0x08, 0x00, // length = 8
        0x03, 0x00, // type = 3 = IFLA_XDP_FLAGS
        0x00, 0x00, 0x00, 0x00, // empty
        0x08, 0x00, // length = 8
        0x04, 0x00, // type = 4 = IFLA_XDP_PROG_ID
        0x67, 0x00, 0x00, 0x00, // 103
        0x08, 0x00, // length = 8
        0x05, 0x00, // type = 5 = IFLA_XDP_DRV_PROG_ID
        0x65, 0x00, 0x00, 0x00, // 101
        0x08, 0x00, // length = 8
        0x06, 0x00, // type = 6 = IFLA_XDP_DRV_SKB_ID
        0x65, 0x00, 0x00, 0x00, // 101
        0x08, 0x00, // length = 8
        0x07, 0x00, // type = 7 = IFLA_XDP_DRV_HW_ID
        0x65, 0x00, 0x00, 0x00, // 101
        0x08, 0x00, // length = 8
        0x08, 0x00, // type = 8 = IFLA_XDP_DRV_EXPECTED_FD
        0xA1, 0x74, 0x00, 0x00, // 29857
        0x08, 0x00, // length = 8
        0xfc, 0x00, // type = 252 = random number/unknown type
        0xA1, 0x74, 0x00, 0x00, // 29857
        0x06, 0x00, // length = 6
        0xfb, 0x00, // type = 251 = random number/unknown type
        0xaa, 0xab, // 29857
        0x00, 0x00, // padding
    ];

#[test]
fn parse_xdp() {
    let nla = NlaBuffer::new_checked(&XDP[..]).unwrap();
    let parsed = VecLinkXdp::parse(&nla).unwrap().0;
    let expected = vec![
        LinkXdp::Fd(29856),
        LinkXdp::Flags(0),
        LinkXdp::ProgId(103),
        LinkXdp::DrvProgId(101),
        LinkXdp::SkbProgId(101),
        LinkXdp::HwProgId(101),
        LinkXdp::ExpectedFd(29857),
        LinkXdp::Other(
            DefaultNla::parse(&NlaBuffer::new(&XDP[56..64])).unwrap(),
        ),
        LinkXdp::Other(DefaultNla::parse(&NlaBuffer::new(&XDP[64..])).unwrap()),
    ];
    assert_eq!(expected, parsed);
}

#[test]
fn emit_xdp() {
    let nlas = vec![
        LinkXdp::Fd(29856),
        LinkXdp::Flags(0),
        LinkXdp::ProgId(103),
        LinkXdp::DrvProgId(101),
        LinkXdp::SkbProgId(101),
        LinkXdp::HwProgId(101),
        LinkXdp::ExpectedFd(29857),
        LinkXdp::Other(
            DefaultNla::parse(&NlaBuffer::new(&XDP[56..64])).unwrap(),
        ),
        LinkXdp::Other(DefaultNla::parse(&NlaBuffer::new(&XDP[64..])).unwrap()),
    ];
    assert_eq!(nlas.as_slice().buffer_len(), XDP.len());

    let mut vec = vec![0xff; XDP.len()];
    nlas.as_slice().emit(&mut vec);
    assert_eq!(&vec[..], &XDP[..]);
}
