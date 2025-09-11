// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    parsers::{parse_u32, parse_u64},
    DecodeError, Parseable,
};

const NDTPA_IFINDEX: u16 = 1;
const NDTPA_REFCNT: u16 = 2;
const NDTPA_REACHABLE_TIME: u16 = 3;
const NDTPA_BASE_REACHABLE_TIME: u16 = 4;
const NDTPA_RETRANS_TIME: u16 = 5;
const NDTPA_GC_STALETIME: u16 = 6;
const NDTPA_DELAY_PROBE_TIME: u16 = 7;
const NDTPA_QUEUE_LEN: u16 = 8;
const NDTPA_APP_PROBES: u16 = 9;
const NDTPA_UCAST_PROBES: u16 = 10;
const NDTPA_MCAST_PROBES: u16 = 11;
const NDTPA_ANYCAST_DELAY: u16 = 12;
const NDTPA_PROXY_DELAY: u16 = 13;
const NDTPA_PROXY_QLEN: u16 = 14;
const NDTPA_LOCKTIME: u16 = 15;
const NDTPA_QUEUE_LENBYTES: u16 = 16;
const NDTPA_MCAST_REPROBES: u16 = 17;
// const NDTPA_PAD: u16 = 18;
const NDTPA_INTERVAL_PROBE_TIME_MS: u16 = 19;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum NeighbourTableParameter {
    Ifindex(u32),
    ReferenceCount(u32),
    ReachableTime(u64),
    BaseReachableTime(u64),
    RetransTime(u64),
    GcStaletime(u64),
    DelayProbeTime(u64),
    QueueLen(u32),
    AppProbes(u32),
    UcastProbes(u32),
    McastProbes(u32),
    AnycastDelay(u64),
    ProxyDelay(u64),
    ProxyQlen(u32),
    Locktime(u64),
    QueueLenbytes(u32),
    McastReprobes(u32),
    IntervalProbeTimeMs(u64),
    Other(DefaultNla),
}

impl Nla for NeighbourTableParameter {
    fn value_len(&self) -> usize {
        match self {
            Self::Ifindex(_)
            | Self::ReferenceCount(_)
            | Self::QueueLen(_)
            | Self::AppProbes(_)
            | Self::UcastProbes(_)
            | Self::McastProbes(_)
            | Self::ProxyQlen(_)
            | Self::QueueLenbytes(_)
            | Self::McastReprobes(_) => 4,

            Self::ReachableTime(_)
            | Self::BaseReachableTime(_)
            | Self::RetransTime(_)
            | Self::GcStaletime(_)
            | Self::DelayProbeTime(_)
            | Self::AnycastDelay(_)
            | Self::ProxyDelay(_)
            | Self::Locktime(_)
            | Self::IntervalProbeTimeMs(_) => 8,

            Self::Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Ifindex(v)
            | Self::ReferenceCount(v)
            | Self::QueueLen(v)
            | Self::AppProbes(v)
            | Self::UcastProbes(v)
            | Self::McastProbes(v)
            | Self::ProxyQlen(v)
            | Self::QueueLenbytes(v)
            | Self::McastReprobes(v) => NativeEndian::write_u32(buffer, *v),

            Self::ReachableTime(v)
            | Self::BaseReachableTime(v)
            | Self::RetransTime(v)
            | Self::GcStaletime(v)
            | Self::DelayProbeTime(v)
            | Self::AnycastDelay(v)
            | Self::ProxyDelay(v)
            | Self::Locktime(v)
            | Self::IntervalProbeTimeMs(v) => {
                NativeEndian::write_u64(buffer, *v)
            }

            Self::Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Ifindex(_) => NDTPA_IFINDEX,
            Self::ReferenceCount(_) => NDTPA_REFCNT,
            Self::ReachableTime(_) => NDTPA_REACHABLE_TIME,
            Self::BaseReachableTime(_) => NDTPA_BASE_REACHABLE_TIME,
            Self::RetransTime(_) => NDTPA_RETRANS_TIME,
            Self::GcStaletime(_) => NDTPA_GC_STALETIME,
            Self::DelayProbeTime(_) => NDTPA_DELAY_PROBE_TIME,
            Self::QueueLen(_) => NDTPA_QUEUE_LEN,
            Self::AppProbes(_) => NDTPA_APP_PROBES,
            Self::UcastProbes(_) => NDTPA_UCAST_PROBES,
            Self::McastProbes(_) => NDTPA_MCAST_PROBES,
            Self::AnycastDelay(_) => NDTPA_ANYCAST_DELAY,
            Self::ProxyDelay(_) => NDTPA_PROXY_DELAY,
            Self::ProxyQlen(_) => NDTPA_PROXY_QLEN,
            Self::Locktime(_) => NDTPA_LOCKTIME,
            Self::QueueLenbytes(_) => NDTPA_QUEUE_LENBYTES,
            Self::McastReprobes(_) => NDTPA_MCAST_REPROBES,
            Self::IntervalProbeTimeMs(_) => NDTPA_INTERVAL_PROBE_TIME_MS,
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for NeighbourTableParameter
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            NDTPA_IFINDEX => {
                Self::Ifindex(parse_u32(payload).context(format!(
                    "invalid NDTPA_IFINDEX value {payload:?}"
                ))?)
            }
            NDTPA_REFCNT => {
                Self::ReferenceCount(parse_u32(payload).context(format!(
                    "invalid NDTPA_REFCNT value {payload:?}"
                ))?)
            }
            NDTPA_REACHABLE_TIME => {
                Self::ReachableTime(parse_u64(payload).context(format!(
                    "invalid NDTPA_REACHABLE_TIME value {payload:?}"
                ))?)
            }
            NDTPA_BASE_REACHABLE_TIME => {
                Self::BaseReachableTime(parse_u64(payload).context(format!(
                    "invalid NDTPA_BASE_REACHABLE_TIME value {payload:?}"
                ))?)
            }
            NDTPA_RETRANS_TIME => {
                Self::RetransTime(parse_u64(payload).context(format!(
                    "invalid NDTPA_RETRANS_TIME value {payload:?}"
                ))?)
            }
            NDTPA_GC_STALETIME => {
                Self::GcStaletime(parse_u64(payload).context(format!(
                    "invalid NDTPA_GC_STALE_TIME value {payload:?}"
                ))?)
            }
            NDTPA_DELAY_PROBE_TIME => {
                Self::DelayProbeTime(parse_u64(payload).context(format!(
                    "invalid NDTPA_DELAY_PROBE_TIME value {payload:?}"
                ))?)
            }
            NDTPA_QUEUE_LEN => Self::QueueLen(parse_u32(payload).context(
                format!("invalid NDTPA_QUEUE_LEN value {payload:?}"),
            )?),
            NDTPA_APP_PROBES => Self::AppProbes(parse_u32(payload).context(
                format!("invalid NDTPA_APP_PROBES value {payload:?}"),
            )?),
            NDTPA_UCAST_PROBES => {
                Self::UcastProbes(parse_u32(payload).context(format!(
                    "invalid NDTPA_UCAST_PROBES value {payload:?}"
                ))?)
            }
            NDTPA_MCAST_PROBES => {
                Self::McastProbes(parse_u32(payload).context(format!(
                    "invalid NDTPA_MCAST_PROBES value {payload:?}"
                ))?)
            }
            NDTPA_ANYCAST_DELAY => {
                Self::AnycastDelay(parse_u64(payload).context(format!(
                    "invalid NDTPA_ANYCAST_DELAY value {payload:?}"
                ))?)
            }
            NDTPA_PROXY_DELAY => Self::ProxyDelay(parse_u64(payload).context(
                format!("invalid NDTPA_PROXY_DELAY value {payload:?}"),
            )?),
            NDTPA_PROXY_QLEN => Self::ProxyQlen(parse_u32(payload).context(
                format!("invalid NDTPA_PROXY_QLEN value {payload:?}"),
            )?),
            NDTPA_LOCKTIME => Self::Locktime(parse_u64(payload).context(
                format!("invalid NDTPA_LOCKTIME value {payload:?}"),
            )?),
            NDTPA_QUEUE_LENBYTES => {
                Self::QueueLenbytes(parse_u32(payload).context(format!(
                    "invalid NDTPA_QUEUE_LENBYTES value {payload:?}"
                ))?)
            }
            NDTPA_MCAST_REPROBES => {
                Self::McastReprobes(parse_u32(payload).context(format!(
                    "invalid NDTPA_MCAST_PROBES value {payload:?}"
                ))?)
            }
            NDTPA_INTERVAL_PROBE_TIME_MS => Self::IntervalProbeTimeMs(
                parse_u64(payload).context(format!(
                    "invalid NDTPA_INTERVAL_PROBE_TIME_MS value {payload:?}"
                ))?,
            ),
            _ => Self::Other(DefaultNla::parse(buf).context(format!(
                "invalid NDTA_PARMS attribute {payload:?}"
            ))?),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct VecNeighbourTableParameter(
    pub(crate) Vec<NeighbourTableParameter>,
);

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for VecNeighbourTableParameter
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut nlas = vec![];
        let err = "invalid NDTA_PARMS attribute";
        for nla in NlasIterator::new(buf.into_inner()) {
            let nla = nla.context(err)?;
            nlas.push(NeighbourTableParameter::parse(&nla).context(err)?);
        }
        Ok(Self(nlas))
    }
}
