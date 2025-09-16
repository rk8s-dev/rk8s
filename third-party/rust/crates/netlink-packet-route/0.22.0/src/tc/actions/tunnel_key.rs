// SPDX-License-Identifier: MIT

use crate::ip::{parse_ipv4_addr, parse_ipv6_addr};
use anyhow::Context;
/// set tunnel key
///
/// The set_tunnel action allows to set tunnel encap applied
/// at the last stage of action processing
use byteorder::{BigEndian, ByteOrder};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_u16_be, parse_u32_be, parse_u8},
    traits::{Emitable, Parseable},
    DecodeError,
};
use std::net::{Ipv4Addr, Ipv6Addr};

use super::{
    TcActionGeneric, TcActionGenericBuffer, Tcf, TcfBuffer, TC_TCF_BUF_LEN,
};

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct TcActionTunnelKey {}
impl TcActionTunnelKey {
    pub const KIND: &'static str = "tunnel_key";
}

const TCA_TUNNEL_KEY_TM: u16 = 1;
const TCA_TUNNEL_KEY_PARMS: u16 = 2;
const TCA_TUNNEL_KEY_ENC_IPV4_SRC: u16 = 3;
const TCA_TUNNEL_KEY_ENC_IPV4_DST: u16 = 4;
const TCA_TUNNEL_KEY_ENC_IPV6_SRC: u16 = 5;
const TCA_TUNNEL_KEY_ENC_IPV6_DST: u16 = 6;
const TCA_TUNNEL_KEY_ENC_KEY_ID: u16 = 7;
// const TCA_TUNNEL_KEY_PAD: u16 = 8;
const TCA_TUNNEL_KEY_ENC_DST_PORT: u16 = 9;
const TCA_TUNNEL_KEY_NO_CSUM: u16 = 10;
// const TCA_TUNNEL_KEY_ENC_OPTS: u16 = 11;
const TCA_TUNNEL_KEY_ENC_TOS: u16 = 12;
const TCA_TUNNEL_KEY_ENC_TTL: u16 = 13;
// const TCA_TUNNEL_KEY_NO_FRAG: u16 = 14;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcActionTunnelKeyOption {
    Tm(Tcf),
    Parms(TcTunnelKey),
    EncIpv4Src(Ipv4Addr),
    EncIpv4Dst(Ipv4Addr),
    EncIpv6Src(Ipv6Addr),
    EncIpv6Dst(Ipv6Addr),
    EncKeyId(u32),
    EncDstPort(u16),
    EncTos(u8),
    EncTtl(u8),
    NoCsum(bool),
    Other(DefaultNla),
}

impl Nla for TcActionTunnelKeyOption {
    fn value_len(&self) -> usize {
        match self {
            Self::Tm(_) => TC_TCF_BUF_LEN,
            Self::Parms(_) => TC_TUNNEL_KEY_BUF_LEN,
            Self::EncIpv4Src(_) | Self::EncIpv4Dst(_) => 4,
            Self::EncIpv6Src(_) | Self::EncIpv6Dst(_) => 16,
            Self::EncKeyId(_) => 4,
            Self::EncDstPort(_) => 2,
            Self::EncTos(_) | Self::EncTtl(_) => 1,
            Self::NoCsum(_) => 1,
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Tm(p) => p.emit(buffer),
            Self::Parms(p) => p.emit(buffer),
            Self::EncIpv4Src(ip) | Self::EncIpv4Dst(ip) => {
                buffer.copy_from_slice(&ip.octets())
            }
            Self::EncIpv6Src(ip) | Self::EncIpv6Dst(ip) => {
                buffer.copy_from_slice(&ip.octets())
            }
            Self::EncKeyId(i) => BigEndian::write_u32(buffer, *i),
            Self::EncDstPort(i) => BigEndian::write_u16(buffer, *i),
            Self::EncTos(i) => buffer[0] = *i,
            Self::EncTtl(i) => buffer[0] = *i,
            Self::NoCsum(i) => buffer[0] = *i as u8,
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }
    fn kind(&self) -> u16 {
        match self {
            Self::Tm(_) => TCA_TUNNEL_KEY_TM,
            Self::Parms(_) => TCA_TUNNEL_KEY_PARMS,
            Self::EncIpv4Src(_) => TCA_TUNNEL_KEY_ENC_IPV4_SRC,
            Self::EncIpv4Dst(_) => TCA_TUNNEL_KEY_ENC_IPV4_DST,
            Self::EncIpv6Src(_) => TCA_TUNNEL_KEY_ENC_IPV6_SRC,
            Self::EncIpv6Dst(_) => TCA_TUNNEL_KEY_ENC_IPV6_DST,
            Self::EncKeyId(_) => TCA_TUNNEL_KEY_ENC_KEY_ID,
            Self::EncDstPort(_) => TCA_TUNNEL_KEY_ENC_DST_PORT,
            Self::EncTos(_) => TCA_TUNNEL_KEY_ENC_TOS,
            Self::EncTtl(_) => TCA_TUNNEL_KEY_ENC_TTL,
            Self::NoCsum(_) => TCA_TUNNEL_KEY_NO_CSUM,
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for TcActionTunnelKeyOption
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            TCA_TUNNEL_KEY_TM => {
                Self::Tm(Tcf::parse(&TcfBuffer::new_checked(payload)?)?)
            }
            TCA_TUNNEL_KEY_PARMS => Self::Parms(TcTunnelKey::parse(
                &TcTunnelKeyBuffer::new_checked(payload)?,
            )?),
            TCA_TUNNEL_KEY_ENC_IPV4_SRC => Self::EncIpv4Src(
                parse_ipv4_addr(payload)
                    .context("failed to parse TCA_TUNNEL_KEY_ENC_IPV4_SRC")?,
            ),
            TCA_TUNNEL_KEY_ENC_IPV4_DST => Self::EncIpv4Dst(
                parse_ipv4_addr(payload)
                    .context("failed to parse TCA_TUNNEL_KEY_ENC_IPV4_DST")?,
            ),
            TCA_TUNNEL_KEY_ENC_IPV6_SRC => Self::EncIpv6Src(
                parse_ipv6_addr(payload)
                    .context("failed to parse TCA_TUNNEL_KEY_ENC_IPV6_SRC")?,
            ),
            TCA_TUNNEL_KEY_ENC_IPV6_DST => Self::EncIpv6Dst(
                parse_ipv6_addr(payload)
                    .context("failed to parse TCA_TUNNEL_KEY_ENC_IPV6_DST")?,
            ),
            TCA_TUNNEL_KEY_ENC_KEY_ID => Self::EncKeyId(
                parse_u32_be(payload)
                    .context("failed to parse TCA_TUNNEL_KEY_ENC_KEY_ID")?,
            ),
            TCA_TUNNEL_KEY_ENC_DST_PORT => Self::EncDstPort(
                parse_u16_be(payload)
                    .context("failed to parse TCA_TUNNEL_KEY_ENC_DST_PORT")?,
            ),
            TCA_TUNNEL_KEY_ENC_TOS => Self::EncTos(
                parse_u8(payload)
                    .context("failed to parse TCA_TUNNEL_KEY_ENC_TOS")?,
            ),
            TCA_TUNNEL_KEY_ENC_TTL => Self::EncTtl(
                parse_u8(payload)
                    .context("failed to parse TCA_TUNNEL_KEY_ENC_TTL")?,
            ),
            TCA_TUNNEL_KEY_NO_CSUM => Self::NoCsum(
                parse_u8(payload)
                    .context("invalid TCA_TUNNEL_KEY_NO_CSUM value")?
                    != 0,
            ),
            _ => Self::Other(DefaultNla::parse(buf)?),
        })
    }
}

const TC_TUNNEL_KEY_BUF_LEN: usize = TcActionGeneric::BUF_LEN + 4;

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct TcTunnelKey {
    pub generic: TcActionGeneric,
    pub t_action: i32,
}

// kernel struct `tc_tunnel_key`
buffer!(TcTunnelKeyBuffer(TC_TUNNEL_KEY_BUF_LEN) {
    generic: (slice, 0..20),
    t_action: (i32, 20..24),
});

impl Emitable for TcTunnelKey {
    fn buffer_len(&self) -> usize {
        TC_TUNNEL_KEY_BUF_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = TcTunnelKeyBuffer::new(buffer);
        self.generic.emit(packet.generic_mut());
        packet.set_t_action(self.t_action);
    }
}

impl<T: AsRef<[u8]> + ?Sized> Parseable<TcTunnelKeyBuffer<&T>> for TcTunnelKey {
    fn parse(buf: &TcTunnelKeyBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self {
            generic: TcActionGeneric::parse(&TcActionGenericBuffer::new(
                buf.generic(),
            ))?,
            t_action: buf.t_action(),
        })
    }
}
