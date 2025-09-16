// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_string, parse_u32, parse_u64},
    DecodeError, Emitable, Parseable,
};

use super::{
    param::VecNeighbourTableParameter, NeighbourTableConfig,
    NeighbourTableConfigBuffer, NeighbourTableParameter, NeighbourTableStats,
    NeighbourTableStatsBuffer,
};

const NDTA_NAME: u16 = 1;
const NDTA_THRESH1: u16 = 2;
const NDTA_THRESH2: u16 = 3;
const NDTA_THRESH3: u16 = 4;
const NDTA_CONFIG: u16 = 5;
const NDTA_PARMS: u16 = 6;
const NDTA_STATS: u16 = 7;
const NDTA_GC_INTERVAL: u16 = 8;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum NeighbourTableAttribute {
    Parms(Vec<NeighbourTableParameter>),
    Name(String),
    Threshold1(u32),
    Threshold2(u32),
    Threshold3(u32),
    Config(NeighbourTableConfig),
    Stats(NeighbourTableStats),
    GcInterval(u64),
    Other(DefaultNla),
}

impl Nla for NeighbourTableAttribute {
    fn value_len(&self) -> usize {
        match self {
            Self::Parms(v) => v.as_slice().buffer_len(),
            Self::Stats(v) => v.buffer_len(),
            Self::Config(v) => v.buffer_len(),
            // strings: +1 because we need to append a nul byte
            Self::Name(s) => s.len() + 1,
            Self::Threshold1(_) | Self::Threshold2(_) | Self::Threshold3(_) => {
                4
            }
            Self::GcInterval(_) => 8,
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Parms(v) => v.as_slice().emit(buffer),
            Self::Stats(v) => v.emit(buffer),
            Self::Config(v) => v.emit(buffer),
            Self::Name(string) => {
                buffer[..string.len()].copy_from_slice(string.as_bytes());
                buffer[string.len()] = 0;
            }
            Self::GcInterval(value) => NativeEndian::write_u64(buffer, *value),
            Self::Threshold1(value)
            | Self::Threshold2(value)
            | Self::Threshold3(value) => {
                NativeEndian::write_u32(buffer, *value)
            }
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Name(_) => NDTA_NAME,
            Self::Config(_) => NDTA_CONFIG,
            Self::Stats(_) => NDTA_STATS,
            Self::Parms(_) => NDTA_PARMS,
            Self::GcInterval(_) => NDTA_GC_INTERVAL,
            Self::Threshold1(_) => NDTA_THRESH1,
            Self::Threshold2(_) => NDTA_THRESH2,
            Self::Threshold3(_) => NDTA_THRESH3,
            Self::Other(attr) => attr.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for NeighbourTableAttribute
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            NDTA_NAME => Self::Name(
                parse_string(payload).context("invalid NDTA_NAME value")?,
            ),
            NDTA_CONFIG => Self::Config(
                NeighbourTableConfig::parse(
                    &NeighbourTableConfigBuffer::new_checked(payload)
                        .context(format!("invalid NDTA_CONFIG {payload:?}"))?,
                )
                .context(format!("invalid NDTA_CONFIG {payload:?}"))?,
            ),
            NDTA_STATS => Self::Stats(
                NeighbourTableStats::parse(
                    &NeighbourTableStatsBuffer::new_checked(payload)
                        .context(format!("invalid NDTA_STATS {payload:?}"))?,
                )
                .context(format!("invalid NDTA_STATS {payload:?}"))?,
            ),
            NDTA_PARMS => {
                let err = |payload| format!("invalid NDTA_PARMS {payload:?}");
                Self::Parms(
                    VecNeighbourTableParameter::parse(
                        &NlaBuffer::new_checked(payload)
                            .context(err(payload))?,
                    )
                    .context(err(payload))?
                    .0,
                )
            }
            NDTA_GC_INTERVAL => Self::GcInterval(
                parse_u64(payload).context("invalid NDTA_GC_INTERVAL value")?,
            ),
            NDTA_THRESH1 => Self::Threshold1(
                parse_u32(payload).context("invalid NDTA_THRESH1 value")?,
            ),
            NDTA_THRESH2 => Self::Threshold2(
                parse_u32(payload).context("invalid NDTA_THRESH2 value")?,
            ),
            NDTA_THRESH3 => Self::Threshold3(
                parse_u32(payload).context("invalid NDTA_THRESH3 value")?,
            ),
            kind => Self::Other(
                DefaultNla::parse(buf)
                    .context(format!("unknown NLA type {kind}"))?,
            ),
        })
    }
}
