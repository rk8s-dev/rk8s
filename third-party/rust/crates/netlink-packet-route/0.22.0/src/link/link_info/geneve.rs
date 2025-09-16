// SPDX-License-Identifier: MIT

use std::net::{Ipv4Addr, Ipv6Addr};

use anyhow::Context;
use byteorder::{BigEndian, ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_u16_be, parse_u32, parse_u32_be, parse_u8},
    traits::Parseable,
    DecodeError,
};

const IFLA_GENEVE_ID: u16 = 1;
const IFLA_GENEVE_REMOTE: u16 = 2;
const IFLA_GENEVE_TTL: u16 = 3;
const IFLA_GENEVE_TOS: u16 = 4;
const IFLA_GENEVE_PORT: u16 = 5;
const IFLA_GENEVE_COLLECT_METADATA: u16 = 6;
const IFLA_GENEVE_REMOTE6: u16 = 7;
const IFLA_GENEVE_UDP_CSUM: u16 = 8;
const IFLA_GENEVE_UDP_ZERO_CSUM6_TX: u16 = 9;
const IFLA_GENEVE_UDP_ZERO_CSUM6_RX: u16 = 10;
const IFLA_GENEVE_LABEL: u16 = 11;
const IFLA_GENEVE_TTL_INHERIT: u16 = 12;
const IFLA_GENEVE_DF: u16 = 13;
const IFLA_GENEVE_INNER_PROTO_INHERIT: u16 = 14;

const GENEVE_DF_UNSET: u8 = 0;
const GENEVE_DF_SET: u8 = 1;
const GENEVE_DF_INHERIT: u8 = 2;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum GeneveDf {
    Unset,
    Set,
    Inherit,
    Other(u8),
}

impl From<u8> for GeneveDf {
    fn from(d: u8) -> Self {
        match d {
            GENEVE_DF_UNSET => Self::Unset,
            GENEVE_DF_SET => Self::Set,
            GENEVE_DF_INHERIT => Self::Inherit,
            _ => Self::Other(d),
        }
    }
}

impl From<GeneveDf> for u8 {
    fn from(d: GeneveDf) -> Self {
        match d {
            GeneveDf::Unset => GENEVE_DF_UNSET,
            GeneveDf::Set => GENEVE_DF_SET,
            GeneveDf::Inherit => GENEVE_DF_INHERIT,
            GeneveDf::Other(value) => value,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoGeneve {
    Id(u32),
    Remote(Ipv4Addr),
    Remote6(Ipv6Addr),
    Ttl(u8),
    Tos(u8),
    Port(u16),
    CollectMetadata,
    UdpCsum(bool),
    UdpZeroCsum6Tx(bool),
    UdpZeroCsum6Rx(bool),
    Label(u32),
    TtlInherit(bool),
    Df(GeneveDf),
    InnerProtoInherit,
    Other(DefaultNla),
}

impl Nla for InfoGeneve {
    fn value_len(&self) -> usize {
        use self::InfoGeneve::*;
        match self {
            Id(_) | Remote(_) | Label(_) => 4,
            Remote6(_) => 16,
            Ttl(_) | Tos(_) | UdpCsum(_) | UdpZeroCsum6Tx(_)
            | UdpZeroCsum6Rx(_) | TtlInherit(_) | Df(_) => 1,
            Port(_) => 2,
            CollectMetadata | InnerProtoInherit => 0,
            Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        use self::InfoGeneve::*;
        match self {
            Id(value) => NativeEndian::write_u32(buffer, *value),
            Remote(value) => buffer.copy_from_slice(&value.octets()),
            Remote6(value) => buffer.copy_from_slice(&value.octets()),
            Ttl(value) | Tos(value) => buffer[0] = *value,
            Port(value) => BigEndian::write_u16(buffer, *value),
            CollectMetadata | InnerProtoInherit => (),
            UdpCsum(value)
            | UdpZeroCsum6Tx(value)
            | UdpZeroCsum6Rx(value)
            | TtlInherit(value) => buffer[0] = *value as u8,
            Label(value) => BigEndian::write_u32(buffer, *value),
            Df(value) => buffer[0] = (*value).into(),
            Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        use self::InfoGeneve::*;
        match self {
            Id(_) => IFLA_GENEVE_ID,
            Remote(_) => IFLA_GENEVE_REMOTE,
            Remote6(_) => IFLA_GENEVE_REMOTE6,
            Ttl(_) => IFLA_GENEVE_TTL,
            Tos(_) => IFLA_GENEVE_TOS,
            Port(_) => IFLA_GENEVE_PORT,
            CollectMetadata => IFLA_GENEVE_COLLECT_METADATA,
            UdpCsum(_) => IFLA_GENEVE_UDP_CSUM,
            UdpZeroCsum6Tx(_) => IFLA_GENEVE_UDP_ZERO_CSUM6_TX,
            UdpZeroCsum6Rx(_) => IFLA_GENEVE_UDP_ZERO_CSUM6_RX,
            Label(_) => IFLA_GENEVE_LABEL,
            TtlInherit(_) => IFLA_GENEVE_TTL_INHERIT,
            Df(_) => IFLA_GENEVE_DF,
            InnerProtoInherit => IFLA_GENEVE_INNER_PROTO_INHERIT,
            Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoGeneve {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        use self::InfoGeneve::*;
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_GENEVE_ID => {
                Id(parse_u32(payload)
                    .context("invalid IFLA_GENEVE_ID value")?)
            }
            IFLA_GENEVE_REMOTE => {
                if payload.len() == 4 {
                    let mut data = [0u8; 4];
                    data.copy_from_slice(&payload[0..4]);
                    Remote(Ipv4Addr::from(data))
                } else {
                    return Err(DecodeError::from(format!(
                        "Invalid IFLA_GENEVE_REMOTE, got unexpected length \
                        of IPv4 address payload {:?}",
                        payload
                    )));
                }
            }
            IFLA_GENEVE_REMOTE6 => {
                if payload.len() == 16 {
                    let mut data = [0u8; 16];
                    data.copy_from_slice(&payload[0..16]);
                    Remote6(Ipv6Addr::from(data))
                } else {
                    return Err(DecodeError::from(format!(
                        "Invalid IFLA_GENEVE_REMOTE6, got unexpected length \
                        of IPv6 address payload {:?}",
                        payload
                    )));
                }
            }
            IFLA_GENEVE_TTL => {
                Ttl(parse_u8(payload)
                    .context("invalid IFLA_GENEVE_TTL value")?)
            }
            IFLA_GENEVE_TOS => {
                Tos(parse_u8(payload)
                    .context("invalid IFLA_GENEVE_TOS value")?)
            }
            IFLA_GENEVE_PORT => Port(
                parse_u16_be(payload)
                    .context("invalid IFLA_GENEVE_PORT value")?,
            ),
            IFLA_GENEVE_COLLECT_METADATA => CollectMetadata,
            IFLA_GENEVE_UDP_CSUM => UdpCsum(
                parse_u8(payload)
                    .context("invalid IFLA_GENEVE_UDP_CSUM value")?
                    > 0,
            ),
            IFLA_GENEVE_UDP_ZERO_CSUM6_TX => UdpZeroCsum6Tx(
                parse_u8(payload)
                    .context("invalid IFLA_GENEVE_UDP_ZERO_CSUM6_TX value")?
                    > 0,
            ),
            IFLA_GENEVE_UDP_ZERO_CSUM6_RX => UdpZeroCsum6Rx(
                parse_u8(payload)
                    .context("invalid IFLA_GENEVE_UDP_ZERO_CSUM6_RX value")?
                    > 0,
            ),
            IFLA_GENEVE_LABEL => Label(
                parse_u32_be(payload)
                    .context("invalid IFLA_GENEVE_LABEL value")?,
            ),
            IFLA_GENEVE_TTL_INHERIT => TtlInherit(
                parse_u8(payload)
                    .context("invalid IFLA_GENEVE_TTL_INHERIT value")?
                    > 0,
            ),
            IFLA_GENEVE_DF => Df(parse_u8(payload)
                .context("invalid IFLA_GENEVE_DF value")?
                .into()),
            IFLA_GENEVE_INNER_PROTO_INHERIT => InnerProtoInherit,
            kind => Other(
                DefaultNla::parse(buf)
                    .context(format!("unknown NLA type {kind}"))?,
            ),
        })
    }
}
