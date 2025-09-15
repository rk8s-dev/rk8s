// SPDX-License-Identifier: MIT

use std::net::Ipv6Addr;

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    parsers::{parse_u32, parse_u8},
    traits::{Emitable, Parseable},
    DecodeError,
};

use super::super::{
    buffer_tool::expand_buffer_if_small, Icmp6Stats, Icmp6StatsBuffer,
    Inet6CacheInfo, Inet6CacheInfoBuffer, Inet6DevConf, Inet6DevConfBuffer,
    Inet6IfaceFlags, Inet6Stats, Inet6StatsBuffer,
};
use super::{
    inet6_devconf::LINK_INET6_DEV_CONF_LEN, inet6_icmp::ICMP6_STATS_LEN,
    inet6_stats::INET6_STATS_LEN,
};
use crate::ip::parse_ipv6_addr;

const IFLA_INET6_FLAGS: u16 = 1;
const IFLA_INET6_CONF: u16 = 2;
const IFLA_INET6_STATS: u16 = 3;
// No kernel code used IFLA_INET6_MCAST
// const IFLA_INET6_MCAST: u16 = 4;
const IFLA_INET6_CACHEINFO: u16 = 5;
const IFLA_INET6_ICMP6STATS: u16 = 6;
const IFLA_INET6_TOKEN: u16 = 7;
const IFLA_INET6_ADDR_GEN_MODE: u16 = 8;
const IFLA_INET6_RA_MTU: u16 = 9;

#[derive(Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
pub enum AfSpecInet6 {
    //TODO(Gris Ge): Use Vec<enum> for `IFF_UP` and etc
    Flags(Inet6IfaceFlags),
    CacheInfo(Inet6CacheInfo),
    DevConf(Inet6DevConf),
    Stats(Inet6Stats),
    Icmp6Stats(Icmp6Stats),
    Token(Ipv6Addr),
    AddrGenMode(u8),
    RaMtu(u32),
    Other(DefaultNla),
}

pub(crate) struct VecAfSpecInet6(pub(crate) Vec<AfSpecInet6>);

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for VecAfSpecInet6
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut nlas = vec![];
        let err = "Invalid AF_INET6 NLA for IFLA_AF_SPEC(AF_UNSPEC)";
        for nla in NlasIterator::new(buf.into_inner()) {
            let nla = nla.context(err)?;
            nlas.push(AfSpecInet6::parse(&nla).context(err)?);
        }
        Ok(Self(nlas))
    }
}

impl Nla for AfSpecInet6 {
    fn value_len(&self) -> usize {
        use self::AfSpecInet6::*;
        match *self {
            CacheInfo(ref cache_info) => cache_info.buffer_len(),
            DevConf(ref dev_conf) => dev_conf.buffer_len(),
            Stats(ref stats) => stats.buffer_len(),
            Icmp6Stats(ref icmp_stats) => icmp_stats.buffer_len(),
            Flags(_) | RaMtu(_) => 4,
            Token(_) => 16,
            AddrGenMode(_) => 1,
            Other(ref nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        use self::AfSpecInet6::*;
        match *self {
            Flags(ref value) => NativeEndian::write_u32(buffer, value.bits()),
            RaMtu(ref value) => NativeEndian::write_u32(buffer, *value),
            CacheInfo(ref v) => v.emit(buffer),
            DevConf(ref v) => v.emit(buffer),
            Stats(ref v) => v.emit(buffer),
            Icmp6Stats(ref v) => v.emit(buffer),
            Token(v) => buffer.copy_from_slice(&v.octets()),
            AddrGenMode(value) => buffer[0] = value,
            Other(ref nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        use self::AfSpecInet6::*;
        match *self {
            Flags(_) => IFLA_INET6_FLAGS,
            CacheInfo(_) => IFLA_INET6_CACHEINFO,
            DevConf(_) => IFLA_INET6_CONF,
            Stats(_) => IFLA_INET6_STATS,
            Icmp6Stats(_) => IFLA_INET6_ICMP6STATS,
            Token(_) => IFLA_INET6_TOKEN,
            AddrGenMode(_) => IFLA_INET6_ADDR_GEN_MODE,
            RaMtu(_) => IFLA_INET6_RA_MTU,
            Other(ref nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for AfSpecInet6 {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        use self::AfSpecInet6::*;

        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_INET6_FLAGS => Flags(Inet6IfaceFlags::from_bits_retain(
                parse_u32(payload).context("invalid IFLA_INET6_FLAGS value")?,
            )),
            IFLA_INET6_CACHEINFO => CacheInfo(
                Inet6CacheInfo::parse(&Inet6CacheInfoBuffer::new(payload))
                    .context(format!(
                        "invalid IFLA_INET6_CACHEINFO value {:?}",
                        payload
                    ))?,
            ),
            IFLA_INET6_CONF => DevConf(
                Inet6DevConf::parse(&Inet6DevConfBuffer::new(
                    expand_buffer_if_small(
                        payload,
                        LINK_INET6_DEV_CONF_LEN,
                        "IFLA_INET6_CONF",
                    )
                    .as_slice(),
                ))
                .context(format!(
                    "invalid IFLA_INET6_CONF value {:?}",
                    payload
                ))?,
            ),
            IFLA_INET6_STATS => Stats(
                Inet6Stats::parse(&Inet6StatsBuffer::new(
                    expand_buffer_if_small(
                        payload,
                        INET6_STATS_LEN,
                        "IFLA_INET6_STATS",
                    )
                    .as_slice(),
                ))
                .context(format!(
                    "invalid IFLA_INET6_STATS value {:?}",
                    payload
                ))?,
            ),
            IFLA_INET6_ICMP6STATS => Icmp6Stats(
                super::super::Icmp6Stats::parse(&Icmp6StatsBuffer::new(
                    expand_buffer_if_small(
                        payload,
                        ICMP6_STATS_LEN,
                        "IFLA_INET6_ICMP6STATS",
                    )
                    .as_slice(),
                ))
                .context(format!(
                    "invalid IFLA_INET6_ICMP6STATS value {:?}",
                    payload
                ))?,
            ),
            IFLA_INET6_TOKEN => Token(
                parse_ipv6_addr(payload)
                    .context("invalid IFLA_INET6_TOKEN value")?,
            ),
            IFLA_INET6_ADDR_GEN_MODE => AddrGenMode(
                parse_u8(payload)
                    .context("invalid IFLA_INET6_ADDR_GEN_MODE value")?,
            ),
            IFLA_INET6_RA_MTU => RaMtu(
                parse_u32(payload)
                    .context("invalid IFLA_INET6_RA_MTU value")?,
            ),
            kind => Other(DefaultNla::parse(buf).context(format!(
                "unknown AF_INET6 NLA type {kind} for IFLA_AF_SPEC(AF_UNSPEC)"
            ))?),
        })
    }
}
