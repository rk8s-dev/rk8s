// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::parse_u32,
    traits::Parseable,
    DecodeError,
};

const IFLA_XFRM_LINK: u16 = 1;
const IFLA_XFRM_IF_ID: u16 = 2;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoXfrm {
    Link(u32),
    IfId(u32),
    Other(DefaultNla),
}

impl Nla for InfoXfrm {
    fn value_len(&self) -> usize {
        use self::InfoXfrm::*;
        match self {
            Link(_) => 4,
            IfId(_) => 4,
            Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        use self::InfoXfrm::*;
        match self {
            Link(value) => NativeEndian::write_u32(buffer, *value),
            IfId(value) => NativeEndian::write_u32(buffer, *value),
            Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        use self::InfoXfrm::*;
        match self {
            Link(_) => IFLA_XFRM_LINK,
            IfId(_) => IFLA_XFRM_IF_ID,
            Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoXfrm {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        use self::InfoXfrm::*;
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_XFRM_LINK => Link(
                parse_u32(payload).context("invalid IFLA_XFRM_LINK value")?,
            ),
            IFLA_XFRM_IF_ID => IfId(
                parse_u32(payload).context("invalid IFLA_XFRM_IF_ID value")?,
            ),
            kind => Other(DefaultNla::parse(buf).context(format!(
                "unknown NLA type {kind} for IFLA_INFO_DATA(xfrm)"
            ))?),
        })
    }
}
