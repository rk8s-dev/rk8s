// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::parse_u32,
    DecodeError, Parseable,
};

const IFLA_PROTO_DOWN_REASON_MASK: u16 = 1;
const IFLA_PROTO_DOWN_REASON_VALUE: u16 = 2;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum LinkProtocolDownReason {
    Value(u32),
    Mask(u32),
    Other(DefaultNla),
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for LinkProtocolDownReason
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_PROTO_DOWN_REASON_MASK => {
                Self::Mask(parse_u32(payload).context(format!(
                    "invalid IFLA_PROTO_DOWN_REASON_MASK {payload:?}"
                ))?)
            }
            IFLA_PROTO_DOWN_REASON_VALUE => {
                Self::Value(parse_u32(payload).context(format!(
                    "invalid IFLA_PROTO_DOWN_REASON_MASK {payload:?}"
                ))?)
            }
            kind => Self::Other(DefaultNla::parse(buf).context(format!(
                "unknown NLA type {kind} for IFLA_PROTO_DOWN_REASON: \
                {payload:?}"
            ))?),
        })
    }
}

impl Nla for LinkProtocolDownReason {
    fn kind(&self) -> u16 {
        match self {
            Self::Value(_) => IFLA_PROTO_DOWN_REASON_VALUE,
            Self::Mask(_) => IFLA_PROTO_DOWN_REASON_MASK,
            Self::Other(v) => v.kind(),
        }
    }

    fn value_len(&self) -> usize {
        match self {
            Self::Value(_) | Self::Mask(_) => 4,
            Self::Other(v) => v.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Value(v) | Self::Mask(v) => {
                NativeEndian::write_u32(buffer, *v)
            }
            Self::Other(v) => v.emit_value(buffer),
        }
    }
}
