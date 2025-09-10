// SPDX-License-Identifier: MIT

use std::net::{Ipv4Addr, Ipv6Addr};

use anyhow::Context;
use byteorder::{BigEndian, ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_u16_be, parse_u32, parse_u8},
    traits::Parseable,
    DecodeError,
};

const IFLA_VXLAN_ID: u16 = 1;
const IFLA_VXLAN_GROUP: u16 = 2;
const IFLA_VXLAN_LINK: u16 = 3;
const IFLA_VXLAN_LOCAL: u16 = 4;
const IFLA_VXLAN_TTL: u16 = 5;
const IFLA_VXLAN_TOS: u16 = 6;
const IFLA_VXLAN_LEARNING: u16 = 7;
const IFLA_VXLAN_AGEING: u16 = 8;
const IFLA_VXLAN_LIMIT: u16 = 9;
const IFLA_VXLAN_PORT_RANGE: u16 = 10;
const IFLA_VXLAN_PROXY: u16 = 11;
const IFLA_VXLAN_RSC: u16 = 12;
const IFLA_VXLAN_L2MISS: u16 = 13;
const IFLA_VXLAN_L3MISS: u16 = 14;
const IFLA_VXLAN_PORT: u16 = 15;
const IFLA_VXLAN_GROUP6: u16 = 16;
const IFLA_VXLAN_LOCAL6: u16 = 17;
const IFLA_VXLAN_UDP_CSUM: u16 = 18;
const IFLA_VXLAN_UDP_ZERO_CSUM6_TX: u16 = 19;
const IFLA_VXLAN_UDP_ZERO_CSUM6_RX: u16 = 20;
const IFLA_VXLAN_REMCSUM_TX: u16 = 21;
const IFLA_VXLAN_REMCSUM_RX: u16 = 22;
const IFLA_VXLAN_GBP: u16 = 23;
const IFLA_VXLAN_REMCSUM_NOPARTIAL: u16 = 24;
const IFLA_VXLAN_COLLECT_METADATA: u16 = 25;
const IFLA_VXLAN_LABEL: u16 = 26;
const IFLA_VXLAN_GPE: u16 = 27;
const IFLA_VXLAN_TTL_INHERIT: u16 = 28;
const IFLA_VXLAN_DF: u16 = 29;
const IFLA_VXLAN_VNIFILTER: u16 = 30;
const IFLA_VXLAN_LOCALBYPASS: u16 = 31;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoVxlan {
    Id(u32),
    Group(Ipv4Addr),
    Group6(Ipv6Addr),
    Link(u32),
    Local(Ipv4Addr),
    Local6(Ipv6Addr),
    Tos(u8),
    Ttl(u8),
    Label(u32),
    Learning(bool),
    Ageing(u32),
    Limit(u32),
    PortRange((u16, u16)),
    Proxy(bool),
    Rsc(bool),
    L2Miss(bool),
    L3Miss(bool),
    CollectMetadata(bool),
    Port(u16),
    UDPCsum(bool),
    UDPZeroCsumTX(bool),
    UDPZeroCsumRX(bool),
    RemCsumTX(bool),
    RemCsumRX(bool),
    Gbp(bool),
    Gpe(bool),
    RemCsumNoPartial(bool),
    TtlInherit(bool),
    Df(u8),
    Vnifilter(bool),
    Localbypass(bool),
    Other(DefaultNla),
}

impl Nla for InfoVxlan {
    fn value_len(&self) -> usize {
        match self {
            Self::Tos(_)
            | Self::Ttl(_)
            | Self::Learning(_)
            | Self::Proxy(_)
            | Self::Rsc(_)
            | Self::L2Miss(_)
            | Self::L3Miss(_)
            | Self::CollectMetadata(_)
            | Self::UDPCsum(_)
            | Self::UDPZeroCsumTX(_)
            | Self::UDPZeroCsumRX(_)
            | Self::RemCsumTX(_)
            | Self::RemCsumRX(_)
            | Self::TtlInherit(_)
            | Self::Df(_)
            | Self::Vnifilter(_)
            | Self::Localbypass(_) => 1,
            Self::Gbp(_) | Self::Gpe(_) | Self::RemCsumNoPartial(_) => 0,
            Self::Port(_) => 2,
            Self::Id(_)
            | Self::Label(_)
            | Self::Link(_)
            | Self::Ageing(_)
            | Self::Limit(_)
            | Self::PortRange(_)
            | Self::Group(_)
            | Self::Local(_) => 4,
            Self::Group6(_) | Self::Local6(_) => 16,
            Self::Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Id(value)
            | Self::Label(value)
            | Self::Link(value)
            | Self::Ageing(value)
            | Self::Limit(value) => NativeEndian::write_u32(buffer, *value),
            Self::Gbp(_value)
            | Self::Gpe(_value)
            | Self::RemCsumNoPartial(_value) => (),
            Self::Tos(value) | Self::Ttl(value) | Self::Df(value) => {
                buffer[0] = *value
            }
            Self::Vnifilter(value)
            | Self::Localbypass(value)
            | Self::Learning(value)
            | Self::Proxy(value)
            | Self::Rsc(value)
            | Self::L2Miss(value)
            | Self::L3Miss(value)
            | Self::CollectMetadata(value)
            | Self::UDPCsum(value)
            | Self::UDPZeroCsumTX(value)
            | Self::UDPZeroCsumRX(value)
            | Self::RemCsumTX(value)
            | Self::RemCsumRX(value)
            | Self::TtlInherit(value) => buffer[0] = *value as u8,
            Self::Group(value) | Self::Local(value) => {
                buffer.copy_from_slice(&value.octets())
            }
            Self::Group6(value) | Self::Local6(value) => {
                buffer.copy_from_slice(&value.octets())
            }
            Self::Port(value) => BigEndian::write_u16(buffer, *value),
            Self::PortRange(range) => {
                BigEndian::write_u16(buffer, range.0);
                BigEndian::write_u16(&mut buffer[2..], range.1)
            }
            Self::Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Id(_) => IFLA_VXLAN_ID,
            Self::Group(_) => IFLA_VXLAN_GROUP,
            Self::Group6(_) => IFLA_VXLAN_GROUP6,
            Self::Link(_) => IFLA_VXLAN_LINK,
            Self::Local(_) => IFLA_VXLAN_LOCAL,
            Self::Local6(_) => IFLA_VXLAN_LOCAL6,
            Self::Tos(_) => IFLA_VXLAN_TOS,
            Self::Ttl(_) => IFLA_VXLAN_TTL,
            Self::Label(_) => IFLA_VXLAN_LABEL,
            Self::Learning(_) => IFLA_VXLAN_LEARNING,
            Self::Ageing(_) => IFLA_VXLAN_AGEING,
            Self::Limit(_) => IFLA_VXLAN_LIMIT,
            Self::PortRange(_) => IFLA_VXLAN_PORT_RANGE,
            Self::Proxy(_) => IFLA_VXLAN_PROXY,
            Self::Rsc(_) => IFLA_VXLAN_RSC,
            Self::L2Miss(_) => IFLA_VXLAN_L2MISS,
            Self::L3Miss(_) => IFLA_VXLAN_L3MISS,
            Self::CollectMetadata(_) => IFLA_VXLAN_COLLECT_METADATA,
            Self::Port(_) => IFLA_VXLAN_PORT,
            Self::UDPCsum(_) => IFLA_VXLAN_UDP_CSUM,
            Self::UDPZeroCsumTX(_) => IFLA_VXLAN_UDP_ZERO_CSUM6_TX,
            Self::UDPZeroCsumRX(_) => IFLA_VXLAN_UDP_ZERO_CSUM6_RX,
            Self::RemCsumTX(_) => IFLA_VXLAN_REMCSUM_TX,
            Self::RemCsumRX(_) => IFLA_VXLAN_REMCSUM_RX,
            Self::Gbp(_) => IFLA_VXLAN_GBP,
            Self::Gpe(_) => IFLA_VXLAN_GPE,
            Self::RemCsumNoPartial(_) => IFLA_VXLAN_REMCSUM_NOPARTIAL,
            Self::TtlInherit(_) => IFLA_VXLAN_TTL_INHERIT,
            Self::Df(_) => IFLA_VXLAN_DF,
            Self::Vnifilter(_) => IFLA_VXLAN_VNIFILTER,
            Self::Localbypass(_) => IFLA_VXLAN_LOCALBYPASS,
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoVxlan {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_VXLAN_ID => {
                Self::Id(parse_u32(payload).context("invalid IFLA_VXLAN_ID value")?)
            }
            IFLA_VXLAN_GROUP => {
                if payload.len() == 4 {
                    let mut data = [0u8; 4];
                    data.copy_from_slice(&payload[0..4]);
                    Self::Group(Ipv4Addr::from(data))
                } else {
                    return Err(DecodeError::from(format!(
                        "Invalid IFLA_VXLAN_GROUP, got unexpected length \
                        of IPv4 address payload {:?}",
                        payload
                    )));
                }
            }
            IFLA_VXLAN_LOCAL => {
                if payload.len() == 4 {
                    let mut data = [0u8; 4];
                    data.copy_from_slice(&payload[0..4]);
                    Self::Local(Ipv4Addr::from(data))
                } else {
                    return Err(DecodeError::from(format!(
                        "Invalid IFLA_VXLAN_LOCAL, got unexpected length \
                        of IPv4 address payload {:?}",
                        payload
                    )));
                }
            }
            IFLA_VXLAN_GROUP6 => {
                if payload.len() == 16 {
                    let mut data = [0u8; 16];
                    data.copy_from_slice(&payload[0..16]);
                    Self::Group6(Ipv6Addr::from(data))
                } else {
                    return Err(DecodeError::from(format!(
                        "Invalid IFLA_VXLAN_GROUP6, got unexpected length \
                        of IPv6 address payload {:?}",
                        payload
                    )));
                }
            },
            IFLA_VXLAN_LOCAL6 => {
                if payload.len() == 16 {
                    let mut data = [0u8; 16];
                    data.copy_from_slice(&payload[0..16]);
                    Self::Local6(Ipv6Addr::from(data))
                } else {
                    return Err(DecodeError::from(format!(
                        "Invalid IFLA_VXLAN_LOCAL6, got unexpected length \
                        of IPv6 address payload {:?}",
                        payload
                    )));
                }
            },
            IFLA_VXLAN_LINK => Self::Link(
                parse_u32(payload).context("invalid IFLA_VXLAN_LINK value")?,
            ),
            IFLA_VXLAN_TOS => {
                Self::Tos(parse_u8(payload)
                    .context("invalid IFLA_VXLAN_TOS value")?)
            }
            IFLA_VXLAN_TTL => {
                Self::Ttl(parse_u8(payload)
                    .context("invalid IFLA_VXLAN_TTL value")?)
            }
            IFLA_VXLAN_LABEL => Self::Label(
                parse_u32(payload).context("invalid IFLA_VXLAN_LABEL value")?,
            ),
            IFLA_VXLAN_LEARNING => Self::Learning(
                parse_u8(payload)
                    .context("invalid IFLA_VXLAN_LEARNING value")? > 0,
            ),
            IFLA_VXLAN_AGEING => Self::Ageing(
                parse_u32(payload)
                    .context("invalid IFLA_VXLAN_AGEING value")?,
            ),
            IFLA_VXLAN_LIMIT => Self::Limit(
                parse_u32(payload).context("invalid IFLA_VXLAN_LIMIT value")?,
            ),
            IFLA_VXLAN_PROXY => Self::Proxy(
                parse_u8(payload).context("invalid IFLA_VXLAN_PROXY value")? > 0,
            ),
            IFLA_VXLAN_RSC => {
                Self::Rsc(parse_u8(payload)
                    .context("invalid IFLA_VXLAN_RSC value")?> 0)
            }
            IFLA_VXLAN_L2MISS => Self::L2Miss(
                parse_u8(payload).context("invalid IFLA_VXLAN_L2MISS value")? > 0,
            ),
            IFLA_VXLAN_L3MISS => Self::L3Miss(
                parse_u8(payload).context("invalid IFLA_VXLAN_L3MISS value")? > 0,
            ),
            IFLA_VXLAN_COLLECT_METADATA => Self::CollectMetadata(
                parse_u8(payload)
                    .context("invalid IFLA_VXLAN_COLLECT_METADATA value")? >0,
            ),
            IFLA_VXLAN_PORT_RANGE => {
                let err = "invalid IFLA_VXLAN_PORT value";
                if payload.len() != 4 {
                    return Err(err.into());
                }
                let low = parse_u16_be(&payload[0..2]).context(err)?;
                let high = parse_u16_be(&payload[2..]).context(err)?;
                Self::PortRange((low, high))
            }
            IFLA_VXLAN_PORT => Self::Port(
                parse_u16_be(payload)
                    .context("invalid IFLA_VXLAN_PORT value")?,
            ),
            IFLA_VXLAN_UDP_CSUM => Self::UDPCsum(
                parse_u8(payload)
                    .context("invalid IFLA_VXLAN_UDP_CSUM value")? > 0,
            ),
            IFLA_VXLAN_UDP_ZERO_CSUM6_TX => Self::UDPZeroCsumTX(
                parse_u8(payload)
                    .context("invalid IFLA_VXLAN_UDP_ZERO_CSUM6_TX value")? > 0,
            ),
            IFLA_VXLAN_UDP_ZERO_CSUM6_RX => Self::UDPZeroCsumRX(
                parse_u8(payload)
                    .context("invalid IFLA_VXLAN_UDP_ZERO_CSUM6_RX value")? > 0,
            ),
            IFLA_VXLAN_REMCSUM_TX => Self::RemCsumTX(
                parse_u8(payload)
                    .context("invalid IFLA_VXLAN_REMCSUM_TX value")? > 0,
            ),
            IFLA_VXLAN_REMCSUM_RX => Self::RemCsumRX(
                parse_u8(payload)
                    .context("invalid IFLA_VXLAN_REMCSUM_RX value")? > 0,
            ),
            IFLA_VXLAN_DF => {
                Self::Df(parse_u8(payload).context("invalid IFLA_VXLAN_DF value")?)
            }
            IFLA_VXLAN_GBP => {
                Self::Gbp(true)
            }
            IFLA_VXLAN_GPE => {
                Self::Gpe(true)
            }
            IFLA_VXLAN_REMCSUM_NOPARTIAL => Self::RemCsumNoPartial(true),
            IFLA_VXLAN_TTL_INHERIT => Self::TtlInherit(
                parse_u8(payload)
                    .context("invalid IFLA_VXLAN_TTL_INHERIT value")? > 0,
            ),
            IFLA_VXLAN_VNIFILTER => Self::Vnifilter(
                parse_u8(payload)
                    .context("invalid IFLA_VXLAN_VNIFILTER value")? > 0,
            ),
            IFLA_VXLAN_LOCALBYPASS => Self::Localbypass(
                parse_u8(payload)
                    .context("invalid IFLA_VXLAN_LOCALBYPASS value")? > 0,
            ),
            unknown_kind => Self::Other(DefaultNla::parse(buf).context(format!(
                "Failed to parse IFLA_INFO_DATA(vxlan) NLA type: {unknown_kind} as DefaultNla"
            ))?),
        })
    }
}
