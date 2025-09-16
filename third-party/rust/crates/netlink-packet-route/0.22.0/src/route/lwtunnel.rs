// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    traits::{Emitable, Parseable, ParseableParametrized},
    DecodeError,
};

use super::RouteMplsIpTunnel;

const LWTUNNEL_ENCAP_NONE: u16 = 0;
const LWTUNNEL_ENCAP_MPLS: u16 = 1;
const LWTUNNEL_ENCAP_IP: u16 = 2;
const LWTUNNEL_ENCAP_ILA: u16 = 3;
const LWTUNNEL_ENCAP_IP6: u16 = 4;
const LWTUNNEL_ENCAP_SEG6: u16 = 5;
const LWTUNNEL_ENCAP_BPF: u16 = 6;
const LWTUNNEL_ENCAP_SEG6_LOCAL: u16 = 7;
const LWTUNNEL_ENCAP_RPL: u16 = 8;
const LWTUNNEL_ENCAP_IOAM6: u16 = 9;
const LWTUNNEL_ENCAP_XFRM: u16 = 10;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
#[non_exhaustive]
pub enum RouteLwEnCapType {
    #[default]
    None,
    Mpls,
    Ip,
    Ila,
    Ip6,
    Seg6,
    Bpf,
    Seg6Local,
    Rpl,
    Ioam6,
    Xfrm,
    Other(u16),
}

impl From<u16> for RouteLwEnCapType {
    fn from(d: u16) -> Self {
        match d {
            LWTUNNEL_ENCAP_NONE => Self::None,
            LWTUNNEL_ENCAP_MPLS => Self::Mpls,
            LWTUNNEL_ENCAP_IP => Self::Ip,
            LWTUNNEL_ENCAP_ILA => Self::Ila,
            LWTUNNEL_ENCAP_IP6 => Self::Ip6,
            LWTUNNEL_ENCAP_SEG6 => Self::Seg6,
            LWTUNNEL_ENCAP_BPF => Self::Bpf,
            LWTUNNEL_ENCAP_SEG6_LOCAL => Self::Seg6Local,
            LWTUNNEL_ENCAP_RPL => Self::Rpl,
            LWTUNNEL_ENCAP_IOAM6 => Self::Ioam6,
            LWTUNNEL_ENCAP_XFRM => Self::Xfrm,
            _ => Self::Other(d),
        }
    }
}

impl From<RouteLwEnCapType> for u16 {
    fn from(v: RouteLwEnCapType) -> u16 {
        match v {
            RouteLwEnCapType::None => LWTUNNEL_ENCAP_NONE,
            RouteLwEnCapType::Mpls => LWTUNNEL_ENCAP_MPLS,
            RouteLwEnCapType::Ip => LWTUNNEL_ENCAP_IP,
            RouteLwEnCapType::Ila => LWTUNNEL_ENCAP_ILA,
            RouteLwEnCapType::Ip6 => LWTUNNEL_ENCAP_IP6,
            RouteLwEnCapType::Seg6 => LWTUNNEL_ENCAP_SEG6,
            RouteLwEnCapType::Bpf => LWTUNNEL_ENCAP_BPF,
            RouteLwEnCapType::Seg6Local => LWTUNNEL_ENCAP_SEG6_LOCAL,
            RouteLwEnCapType::Rpl => LWTUNNEL_ENCAP_RPL,
            RouteLwEnCapType::Ioam6 => LWTUNNEL_ENCAP_IOAM6,
            RouteLwEnCapType::Xfrm => LWTUNNEL_ENCAP_XFRM,
            RouteLwEnCapType::Other(d) => d,
        }
    }
}

impl Emitable for RouteLwEnCapType {
    fn buffer_len(&self) -> usize {
        2
    }

    fn emit(&self, buffer: &mut [u8]) {
        buffer.copy_from_slice(&(u16::from(*self).to_ne_bytes()))
    }
}

impl std::fmt::Display for RouteLwEnCapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Mpls => write!(f, "mpls"),
            Self::Ip => write!(f, "ip"),
            Self::Ila => write!(f, "ila"),
            Self::Ip6 => write!(f, "ip6"),
            Self::Seg6 => write!(f, "seg6"),
            Self::Bpf => write!(f, "bpf"),
            Self::Seg6Local => write!(f, "seg6_local"),
            Self::Rpl => write!(f, "rpl"),
            Self::Ioam6 => write!(f, "ioam6"),
            Self::Xfrm => write!(f, "xfrm"),
            Self::Other(d) => write!(f, "other({d})"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum RouteLwTunnelEncap {
    Mpls(RouteMplsIpTunnel),
    Other(DefaultNla),
}

impl Nla for RouteLwTunnelEncap {
    fn value_len(&self) -> usize {
        match self {
            Self::Mpls(v) => v.value_len(),
            Self::Other(v) => v.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Mpls(v) => v.emit_value(buffer),
            Self::Other(v) => v.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Mpls(v) => v.kind(),
            Self::Other(v) => v.kind(),
        }
    }
}

impl<'a, T> ParseableParametrized<NlaBuffer<&'a T>, RouteLwEnCapType>
    for RouteLwTunnelEncap
where
    T: AsRef<[u8]> + ?Sized,
{
    fn parse_with_param(
        buf: &NlaBuffer<&'a T>,
        kind: RouteLwEnCapType,
    ) -> Result<Self, DecodeError> {
        Ok(match kind {
            RouteLwEnCapType::Mpls => {
                Self::Mpls(RouteMplsIpTunnel::parse(buf)?)
            }
            _ => Self::Other(DefaultNla::parse(buf)?),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub(crate) struct VecRouteLwTunnelEncap(pub(crate) Vec<RouteLwTunnelEncap>);

impl<'a, T> ParseableParametrized<NlaBuffer<&'a T>, RouteLwEnCapType>
    for VecRouteLwTunnelEncap
where
    T: AsRef<[u8]> + ?Sized,
{
    fn parse_with_param(
        buf: &NlaBuffer<&'a T>,
        kind: RouteLwEnCapType,
    ) -> Result<Self, DecodeError> {
        let mut ret = Vec::new();
        for nla in NlasIterator::new(buf.value()) {
            let nla =
                nla.context(format!("Invalid RTA_ENCAP for kind: {kind}"))?;
            ret.push(RouteLwTunnelEncap::parse_with_param(&nla, kind).context(
                format!("Failed to parse RTA_ENCAP for kind: {kind}",),
            )?)
        }
        Ok(Self(ret))
    }
}
