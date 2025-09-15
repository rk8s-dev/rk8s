// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::parse_u16,
    traits::Parseable,
    DecodeError,
};

const IFLA_IPOIB_PKEY: u16 = 1;
const IFLA_IPOIB_MODE: u16 = 2;
const IFLA_IPOIB_UMCAST: u16 = 3;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoIpoib {
    Pkey(u16),
    Mode(u16),
    UmCast(u16),
    Other(DefaultNla),
}

impl Nla for InfoIpoib {
    fn value_len(&self) -> usize {
        use self::InfoIpoib::*;
        match self {
            Pkey(_) | Mode(_) | UmCast(_) => 2,
            Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        use self::InfoIpoib::*;
        match self {
            Pkey(value) => NativeEndian::write_u16(buffer, *value),
            Mode(value) => NativeEndian::write_u16(buffer, *value),
            UmCast(value) => NativeEndian::write_u16(buffer, *value),
            Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        use self::InfoIpoib::*;
        match self {
            Pkey(_) => IFLA_IPOIB_PKEY,
            Mode(_) => IFLA_IPOIB_MODE,
            UmCast(_) => IFLA_IPOIB_UMCAST,
            Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoIpoib {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        use self::InfoIpoib::*;
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_IPOIB_PKEY => Pkey(
                parse_u16(payload).context("invalid IFLA_IPOIB_PKEY value")?,
            ),
            IFLA_IPOIB_MODE => Mode(
                parse_u16(payload).context("invalid IFLA_IPOIB_MODE value")?,
            ),
            IFLA_IPOIB_UMCAST => UmCast(
                parse_u16(payload)
                    .context("invalid IFLA_IPOIB_UMCAST value")?,
            ),
            kind => Other(DefaultNla::parse(buf).context(format!(
                "unknown NLA type {kind} for IFLA_INFO_DATA(ipoib)"
            ))?),
        })
    }
}
