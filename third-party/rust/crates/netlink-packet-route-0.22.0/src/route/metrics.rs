// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use std::mem::size_of;

use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    parsers::parse_u32,
    traits::Parseable,
    DecodeError,
};

const RTAX_LOCK: u16 = 1;
const RTAX_MTU: u16 = 2;
const RTAX_WINDOW: u16 = 3;
const RTAX_RTT: u16 = 4;
const RTAX_RTTVAR: u16 = 5;
const RTAX_SSTHRESH: u16 = 6;
const RTAX_CWND: u16 = 7;
const RTAX_ADVMSS: u16 = 8;
const RTAX_REORDERING: u16 = 9;
const RTAX_HOPLIMIT: u16 = 10;
const RTAX_INITCWND: u16 = 11;
const RTAX_FEATURES: u16 = 12;
const RTAX_RTO_MIN: u16 = 13;
const RTAX_INITRWND: u16 = 14;
const RTAX_QUICKACK: u16 = 15;
const RTAX_CC_ALGO: u16 = 16;
const RTAX_FASTOPEN_NO_COOKIE: u16 = 17;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum RouteMetric {
    Lock(u32),
    Mtu(u32),
    Window(u32),
    Rtt(u32),
    RttVar(u32),
    SsThresh(u32),
    Cwnd(u32),
    Advmss(u32),
    Reordering(u32),
    Hoplimit(u32),
    InitCwnd(u32),
    Features(u32),
    RtoMin(u32),
    InitRwnd(u32),
    QuickAck(u32),
    CcAlgo(u32),
    FastopenNoCookie(u32),
    Other(DefaultNla),
}

impl Nla for RouteMetric {
    fn value_len(&self) -> usize {
        match self {
            Self::Lock(_)
            | Self::Mtu(_)
            | Self::Window(_)
            | Self::Rtt(_)
            | Self::RttVar(_)
            | Self::SsThresh(_)
            | Self::Cwnd(_)
            | Self::Advmss(_)
            | Self::Reordering(_)
            | Self::Hoplimit(_)
            | Self::InitCwnd(_)
            | Self::Features(_)
            | Self::RtoMin(_)
            | Self::InitRwnd(_)
            | Self::QuickAck(_)
            | Self::CcAlgo(_)
            | Self::FastopenNoCookie(_) => size_of::<u32>(),
            Self::Other(attr) => attr.value_len(),
        }
    }

    #[rustfmt::skip]
    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Lock(value)
                 | Self:: Mtu(value)
                 | Self:: Window(value)
                 | Self:: Rtt(value)
                 | Self:: RttVar(value)
                 | Self:: SsThresh(value)
                 | Self:: Cwnd(value)
                 | Self:: Advmss(value)
                 | Self:: Reordering(value)
                 | Self:: Hoplimit(value)
                 | Self:: InitCwnd(value)
                 | Self:: Features(value)
                 | Self:: RtoMin(value)
                 | Self:: InitRwnd(value)
                 | Self:: QuickAck(value)
                 | Self:: CcAlgo(value)
                 | Self:: FastopenNoCookie(value)
                => NativeEndian::write_u32(buffer, *value),

            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Lock(_) => RTAX_LOCK,
            Self::Mtu(_) => RTAX_MTU,
            Self::Window(_) => RTAX_WINDOW,
            Self::Rtt(_) => RTAX_RTT,
            Self::RttVar(_) => RTAX_RTTVAR,
            Self::SsThresh(_) => RTAX_SSTHRESH,
            Self::Cwnd(_) => RTAX_CWND,
            Self::Advmss(_) => RTAX_ADVMSS,
            Self::Reordering(_) => RTAX_REORDERING,
            Self::Hoplimit(_) => RTAX_HOPLIMIT,
            Self::InitCwnd(_) => RTAX_INITCWND,
            Self::Features(_) => RTAX_FEATURES,
            Self::RtoMin(_) => RTAX_RTO_MIN,
            Self::InitRwnd(_) => RTAX_INITRWND,
            Self::QuickAck(_) => RTAX_QUICKACK,
            Self::CcAlgo(_) => RTAX_CC_ALGO,
            Self::FastopenNoCookie(_) => RTAX_FASTOPEN_NO_COOKIE,
            Self::Other(attr) => attr.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for RouteMetric {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            RTAX_LOCK => Self::Lock(
                parse_u32(payload).context("invalid RTAX_LOCK value")?,
            ),
            RTAX_MTU => {
                Self::Mtu(parse_u32(payload).context("invalid RTAX_MTU value")?)
            }
            RTAX_WINDOW => Self::Window(
                parse_u32(payload).context("invalid RTAX_WINDOW value")?,
            ),
            RTAX_RTT => {
                Self::Rtt(parse_u32(payload).context("invalid RTAX_RTT value")?)
            }
            RTAX_RTTVAR => Self::RttVar(
                parse_u32(payload).context("invalid RTAX_RTTVAR value")?,
            ),
            RTAX_SSTHRESH => Self::SsThresh(
                parse_u32(payload).context("invalid RTAX_SSTHRESH value")?,
            ),
            RTAX_CWND => Self::Cwnd(
                parse_u32(payload).context("invalid RTAX_CWND value")?,
            ),
            RTAX_ADVMSS => Self::Advmss(
                parse_u32(payload).context("invalid RTAX_ADVMSS value")?,
            ),
            RTAX_REORDERING => Self::Reordering(
                parse_u32(payload).context("invalid RTAX_REORDERING value")?,
            ),
            RTAX_HOPLIMIT => Self::Hoplimit(
                parse_u32(payload).context("invalid RTAX_HOPLIMIT value")?,
            ),
            RTAX_INITCWND => Self::InitCwnd(
                parse_u32(payload).context("invalid RTAX_INITCWND value")?,
            ),
            RTAX_FEATURES => Self::Features(
                parse_u32(payload).context("invalid RTAX_FEATURES value")?,
            ),
            RTAX_RTO_MIN => Self::RtoMin(
                parse_u32(payload).context("invalid RTAX_RTO_MIN value")?,
            ),
            RTAX_INITRWND => Self::InitRwnd(
                parse_u32(payload).context("invalid RTAX_INITRWND value")?,
            ),
            RTAX_QUICKACK => Self::QuickAck(
                parse_u32(payload).context("invalid RTAX_QUICKACK value")?,
            ),
            RTAX_CC_ALGO => Self::CcAlgo(
                parse_u32(payload).context("invalid RTAX_CC_ALGO value")?,
            ),
            RTAX_FASTOPEN_NO_COOKIE => Self::FastopenNoCookie(
                parse_u32(payload)
                    .context("invalid RTAX_FASTOPEN_NO_COOKIE value")?,
            ),
            _ => Self::Other(
                DefaultNla::parse(buf)
                    .context("invalid NLA value (unknown type) value")?,
            ),
        })
    }
}

pub(crate) struct VecRouteMetric(pub(crate) Vec<RouteMetric>);

impl<T: AsRef<[u8]> + ?Sized> Parseable<T> for VecRouteMetric {
    fn parse(payload: &T) -> Result<Self, DecodeError> {
        let mut nlas = vec![];
        for nla in NlasIterator::new(payload) {
            let nla = nla.context("Invalid RTA_METRICS")?;
            nlas.push(RouteMetric::parse(&nla).context("Invalid RTA_METRICS")?);
        }
        Ok(Self(nlas))
    }
}
