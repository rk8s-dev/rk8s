// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_mac, parse_u16, parse_u32, parse_u8},
    traits::Parseable,
    DecodeError,
};

// Kernel constant name is IFLA_HSR_SLAVE1
const IFLA_HSR_PORT1: u16 = 1;
// Kernel constant name is IFLA_HSR_SLAVE2
const IFLA_HSR_PORT2: u16 = 2;
const IFLA_HSR_MULTICAST_SPEC: u16 = 3;
const IFLA_HSR_SUPERVISION_ADDR: u16 = 4;
const IFLA_HSR_SEQ_NR: u16 = 5;
const IFLA_HSR_VERSION: u16 = 6;
const IFLA_HSR_PROTOCOL: u16 = 7;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoHsr {
    Port1(u32),
    Port2(u32),
    MulticastSpec(u8),
    SupervisionAddr([u8; 6]),
    Version(u8),
    SeqNr(u16),
    Protocol(HsrProtocol),
    Other(DefaultNla),
}

impl Nla for InfoHsr {
    fn value_len(&self) -> usize {
        use self::InfoHsr::*;
        match self {
            SupervisionAddr(_) => 6,
            Port1(_) | Port2(_) => 4,
            SeqNr(_) => 2,
            MulticastSpec(_) | Version(_) | Protocol(_) => 1,
            Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        use self::InfoHsr::*;
        match self {
            Port1(value) | Port2(value) => {
                NativeEndian::write_u32(buffer, *value)
            }
            MulticastSpec(value) | Version(value) => buffer[0] = *value,
            SeqNr(value) => NativeEndian::write_u16(buffer, *value),
            Protocol(value) => buffer[0] = (*value).into(),
            SupervisionAddr(ref value) => buffer.copy_from_slice(&value[..]),
            Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        use self::InfoHsr::*;
        match self {
            Port1(_) => IFLA_HSR_PORT1,
            Port2(_) => IFLA_HSR_PORT2,
            MulticastSpec(_) => IFLA_HSR_MULTICAST_SPEC,
            SupervisionAddr(_) => IFLA_HSR_SUPERVISION_ADDR,
            SeqNr(_) => IFLA_HSR_SEQ_NR,
            Version(_) => IFLA_HSR_VERSION,
            Protocol(_) => IFLA_HSR_PROTOCOL,
            Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoHsr {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        use self::InfoHsr::*;
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_HSR_PORT1 => Port1(
                parse_u32(payload).context("invalid IFLA_HSR_PORT1 value")?,
            ),
            IFLA_HSR_PORT2 => Port2(
                parse_u32(payload).context("invalid IFLA_HSR_PORT2 value")?,
            ),
            IFLA_HSR_MULTICAST_SPEC => MulticastSpec(
                parse_u8(payload)
                    .context("invalid IFLA_HSR_MULTICAST_SPEC value")?,
            ),
            IFLA_HSR_SUPERVISION_ADDR => SupervisionAddr(
                parse_mac(payload)
                    .context("invalid IFLA_HSR_SUPERVISION_ADDR value")?,
            ),
            IFLA_HSR_SEQ_NR => SeqNr(
                parse_u16(payload).context("invalid IFLA_HSR_SEQ_NR value")?,
            ),
            IFLA_HSR_VERSION => Version(
                parse_u8(payload).context("invalid IFLA_HSR_VERSION value")?,
            ),
            IFLA_HSR_PROTOCOL => Protocol(
                parse_u8(payload)
                    .context("invalid IFLA_HSR_PROTOCOL value")?
                    .into(),
            ),
            kind => Other(
                DefaultNla::parse(buf)
                    .context(format!("unknown NLA type {kind}"))?,
            ),
        })
    }
}

const HSR_PROTOCOL_HSR: u8 = 0;
const HSR_PROTOCOL_PRP: u8 = 1;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
#[repr(u8)]
pub enum HsrProtocol {
    Hsr = HSR_PROTOCOL_HSR,
    Prp = HSR_PROTOCOL_PRP,
    Other(u8),
}

impl From<u8> for HsrProtocol {
    fn from(d: u8) -> Self {
        match d {
            HSR_PROTOCOL_HSR => Self::Hsr,
            HSR_PROTOCOL_PRP => Self::Prp,
            _ => Self::Other(d),
        }
    }
}

impl From<HsrProtocol> for u8 {
    fn from(d: HsrProtocol) -> Self {
        match d {
            HsrProtocol::Hsr => HSR_PROTOCOL_HSR,
            HsrProtocol::Prp => HSR_PROTOCOL_PRP,
            HsrProtocol::Other(value) => value,
        }
    }
}

impl std::fmt::Display for HsrProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hsr => write!(f, "hsr"),
            Self::Prp => write!(f, "prp"),
            Self::Other(d) => write!(f, "{}", d),
        }
    }
}
