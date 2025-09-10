// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    traits::{Parseable, ParseableParametrized},
    DecodeError,
};

use super::{
    TcFilterFlower, TcFilterFlowerOption, TcFilterMatchAll,
    TcFilterMatchAllOption, TcFilterU32, TcFilterU32Option, TcQdiscFqCodel,
    TcQdiscFqCodelOption, TcQdiscIngress, TcQdiscIngressOption,
};

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcOption {
    FqCodel(TcQdiscFqCodelOption),
    // Qdisc specific options
    Ingress(TcQdiscIngressOption),
    // Filter specific options
    Flower(TcFilterFlowerOption),
    U32(TcFilterU32Option),
    // matchall options
    MatchAll(TcFilterMatchAllOption),
    // Other options
    Other(DefaultNla),
}

impl Nla for TcOption {
    fn value_len(&self) -> usize {
        match self {
            Self::FqCodel(u) => u.value_len(),
            Self::Ingress(u) => u.value_len(),
            Self::U32(u) => u.value_len(),
            Self::Flower(u) => u.value_len(),
            Self::MatchAll(m) => m.value_len(),
            Self::Other(o) => o.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::FqCodel(u) => u.emit_value(buffer),
            Self::Ingress(u) => u.emit_value(buffer),
            Self::Flower(u) => u.emit_value(buffer),
            Self::U32(u) => u.emit_value(buffer),
            Self::MatchAll(m) => m.emit_value(buffer),
            Self::Other(o) => o.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::FqCodel(u) => u.kind(),
            Self::Ingress(u) => u.kind(),
            Self::Flower(u) => u.kind(),
            Self::U32(u) => u.kind(),
            Self::MatchAll(m) => m.kind(),
            Self::Other(o) => o.kind(),
        }
    }
}

impl<'a, T> ParseableParametrized<NlaBuffer<&'a T>, &str> for TcOption
where
    T: AsRef<[u8]> + ?Sized,
{
    fn parse_with_param(
        buf: &NlaBuffer<&'a T>,
        kind: &str,
    ) -> Result<Self, DecodeError> {
        Ok(match kind {
            TcQdiscIngress::KIND => {
                Self::Ingress(TcQdiscIngressOption::parse(buf).context(
                    "failed to parse ingress TCA_OPTIONS attributes",
                )?)
            }
            TcFilterFlower::KIND => Self::Flower(
                TcFilterFlowerOption::parse(buf)
                    .context("failed to parse flower TCA_OPTIONS attributes")?,
            ),
            TcQdiscFqCodel::KIND => {
                Self::FqCodel(TcQdiscFqCodelOption::parse(buf).context(
                    "failed to parse fq_codel TCA_OPTIONS attributes",
                )?)
            }
            TcFilterU32::KIND => Self::U32(
                TcFilterU32Option::parse(buf)
                    .context("failed to parse u32 TCA_OPTIONS attributes")?,
            ),
            TcFilterMatchAll::KIND => {
                Self::MatchAll(TcFilterMatchAllOption::parse(buf).context(
                    "failed to parse matchall TCA_OPTIONS attributes",
                )?)
            }
            _ => Self::Other(DefaultNla::parse(buf)?),
        })
    }
}

pub(crate) struct VecTcOption(pub(crate) Vec<TcOption>);

impl<'a, T> ParseableParametrized<NlaBuffer<&'a T>, &str> for VecTcOption
where
    T: AsRef<[u8]> + ?Sized,
{
    fn parse_with_param(
        buf: &NlaBuffer<&'a T>,
        kind: &str,
    ) -> Result<VecTcOption, DecodeError> {
        Ok(match kind {
            TcFilterU32::KIND
            | TcFilterMatchAll::KIND
            | TcFilterFlower::KIND
            | TcQdiscIngress::KIND
            | TcQdiscFqCodel::KIND => {
                let mut nlas = vec![];
                for nla in NlasIterator::new(buf.value()) {
                    let nla = nla.context(format!(
                        "Invalid TCA_OPTIONS for kind: {kind}",
                    ))?;
                    nlas.push(
                        TcOption::parse_with_param(&nla, kind).context(
                            format!(
                                "Failed to parse TCA_OPTIONS for kind: {kind}",
                            ),
                        )?,
                    )
                }
                Self(nlas)
            }
            // Kernel has no guide line or code indicate the scheduler
            // should place a nla_nest here. The `sfq` qdisc kernel code is
            // using single NLA instead nested ones. Hence we are storing
            // unknown Nla as Vec with single item.
            _ => Self(vec![TcOption::Other(DefaultNla::parse(buf)?)]),
        })
    }
}
