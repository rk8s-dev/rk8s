// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_u32, parse_u8},
    traits::{Emitable, Parseable},
    DecodeError,
};

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct TcQdiscFqCodel {}

impl TcQdiscFqCodel {
    pub(crate) const KIND: &'static str = "fq_codel";
}

const TC_FQ_CODEL_QD_STATS_LEN: usize = 36;
const TC_FQ_CODEL_CL_STATS_LEN: usize = 24;

const TCA_FQ_CODEL_XSTATS_QDISC: u32 = 0;
const TCA_FQ_CODEL_XSTATS_CLASS: u32 = 1;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcFqCodelXstats {
    Qdisc(TcFqCodelQdStats),
    Class(TcFqCodelClStats),
    Other(Vec<u8>),
}

impl<T: AsRef<[u8]> + ?Sized> Parseable<T> for TcFqCodelXstats {
    fn parse(buf: &T) -> Result<Self, DecodeError> {
        if buf.as_ref().len() < 4 {
            return Err(DecodeError::from(format!(
                "Invalid TcFqCodelXstats {:?}",
                buf.as_ref()
            )));
        }
        let mut buf_type_bytes = [0; 4];
        buf_type_bytes.copy_from_slice(&buf.as_ref()[0..4]);

        let buf_type = u32::from_ne_bytes(buf_type_bytes);

        match buf_type {
            TCA_FQ_CODEL_XSTATS_QDISC => {
                Ok(Self::Qdisc(TcFqCodelQdStats::parse(
                    &TcFqCodelQdStatsBuffer::new(&buf.as_ref()[4..]),
                )?))
            }
            TCA_FQ_CODEL_XSTATS_CLASS => {
                Ok(Self::Class(TcFqCodelClStats::parse(
                    &TcFqCodelClStatsBuffer::new(&buf.as_ref()[4..]),
                )?))
            }
            _ => Ok(Self::Other(buf.as_ref().to_vec())),
        }
    }
}

impl Emitable for TcFqCodelXstats {
    fn buffer_len(&self) -> usize {
        match self {
            Self::Qdisc(_) => {
                TC_FQ_CODEL_QD_STATS_LEN + std::mem::size_of::<u32>()
            }
            Self::Class(_) => {
                TC_FQ_CODEL_CL_STATS_LEN + std::mem::size_of::<u32>()
            }
            Self::Other(v) => v.len(),
        }
    }

    fn emit(&self, buffer: &mut [u8]) {
        match self {
            Self::Qdisc(v) => {
                buffer[0..4]
                    .copy_from_slice(&TCA_FQ_CODEL_XSTATS_QDISC.to_ne_bytes());
                v.emit(&mut buffer[4..]);
            }
            Self::Class(v) => {
                buffer[0..4]
                    .copy_from_slice(&TCA_FQ_CODEL_XSTATS_CLASS.to_ne_bytes());
                v.emit(&mut buffer[4..]);
            }
            Self::Other(v) => buffer.copy_from_slice(v.as_slice()),
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub struct TcFqCodelQdStats {
    pub maxpacket: u32,
    pub drop_overlimit: u32,
    pub ecn_mark: u32,
    pub new_flow_count: u32,
    pub new_flows_len: u32,
    pub old_flows_len: u32,
    pub ce_mark: u32,
    pub memory_usage: u32,
    pub drop_overmemory: u32,
}

buffer!(TcFqCodelQdStatsBuffer(TC_FQ_CODEL_QD_STATS_LEN) {
    maxpacket: (u32, 0..4),
    drop_overlimit: (u32, 4..8),
    ecn_mark: (u32, 8..12),
    new_flow_count: (u32, 12..16),
    new_flows_len: (u32, 16..20),
    old_flows_len: (u32, 20..24),
    ce_mark: (u32, 24..28),
    memory_usage: (u32, 28..32),
    drop_overmemory: (u32,32..36),
});

impl<T: AsRef<[u8]>> Parseable<TcFqCodelQdStatsBuffer<T>> for TcFqCodelQdStats {
    fn parse(buf: &TcFqCodelQdStatsBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            maxpacket: buf.maxpacket(),
            drop_overlimit: buf.drop_overlimit(),
            ecn_mark: buf.ecn_mark(),
            new_flow_count: buf.new_flow_count(),
            new_flows_len: buf.new_flows_len(),
            old_flows_len: buf.old_flows_len(),
            ce_mark: buf.ce_mark(),
            memory_usage: buf.memory_usage(),
            drop_overmemory: buf.drop_overmemory(),
        })
    }
}

impl Emitable for TcFqCodelQdStats {
    fn buffer_len(&self) -> usize {
        TC_FQ_CODEL_QD_STATS_LEN + std::mem::size_of::<u32>()
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = TcFqCodelQdStatsBuffer::new(buffer);
        buffer.set_maxpacket(self.maxpacket);
        buffer.set_drop_overlimit(self.drop_overlimit);
        buffer.set_ecn_mark(self.ecn_mark);
        buffer.set_new_flow_count(self.new_flow_count);
        buffer.set_new_flows_len(self.new_flows_len);
        buffer.set_old_flows_len(self.old_flows_len);
        buffer.set_ce_mark(self.ce_mark);
        buffer.set_memory_usage(self.memory_usage);
        buffer.set_drop_overmemory(self.drop_overmemory);
    }
}

#[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub struct TcFqCodelClStats {
    deficit: i32,
    ldelay: u32,
    count: u32,
    lastcount: u32,
    dropping: u32,
    drop_next: i32,
}

buffer!(TcFqCodelClStatsBuffer(TC_FQ_CODEL_CL_STATS_LEN) {
    deficit: (i32, 0..4),
    ldelay: (u32,4..8),
    count: (u32, 8..12),
    lastcount: (u32, 12..16),
    dropping: (u32, 16..20),
    drop_next: (i32, 20..24),
});

impl<T: AsRef<[u8]>> Parseable<TcFqCodelClStatsBuffer<T>> for TcFqCodelClStats {
    fn parse(buf: &TcFqCodelClStatsBuffer<T>) -> Result<Self, DecodeError> {
        Ok(Self {
            deficit: buf.deficit(),
            ldelay: buf.ldelay(),
            count: buf.count(),
            lastcount: buf.lastcount(),
            dropping: buf.dropping(),
            drop_next: buf.drop_next(),
        })
    }
}

impl Emitable for TcFqCodelClStats {
    fn buffer_len(&self) -> usize {
        TC_FQ_CODEL_CL_STATS_LEN + std::mem::size_of::<u32>()
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = TcFqCodelClStatsBuffer::new(buffer);
        buffer.set_deficit(self.deficit);
        buffer.set_ldelay(self.ldelay);
        buffer.set_count(self.count);
        buffer.set_lastcount(self.lastcount);
        buffer.set_dropping(self.dropping);
        buffer.set_drop_next(self.drop_next);
    }
}

const TCA_FQ_CODEL_TARGET: u16 = 1;
const TCA_FQ_CODEL_LIMIT: u16 = 2;
const TCA_FQ_CODEL_INTERVAL: u16 = 3;
const TCA_FQ_CODEL_ECN: u16 = 4;
const TCA_FQ_CODEL_FLOWS: u16 = 5;
const TCA_FQ_CODEL_QUANTUM: u16 = 6;
const TCA_FQ_CODEL_CE_THRESHOLD: u16 = 7;
const TCA_FQ_CODEL_DROP_BATCH_SIZE: u16 = 8;
const TCA_FQ_CODEL_MEMORY_LIMIT: u16 = 9;
const TCA_FQ_CODEL_CE_THRESHOLD_SELECTOR: u16 = 10;
const TCA_FQ_CODEL_CE_THRESHOLD_MASK: u16 = 11;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcQdiscFqCodelOption {
    Target(u32),
    Limit(u32),
    Interval(u32),
    Ecn(u32),
    Flows(u32),
    Quantum(u32),
    CeThreshold(u32),
    DropBatchSize(u32),
    MemoryLimit(u32),
    CeThresholdSelector(u8),
    CeThresholdMask(u8),
    Other(DefaultNla),
}

impl Nla for TcQdiscFqCodelOption {
    fn value_len(&self) -> usize {
        match self {
            Self::Target(_)
            | Self::Limit(_)
            | Self::Interval(_)
            | Self::Ecn(_)
            | Self::Flows(_)
            | Self::Quantum(_)
            | Self::CeThreshold(_)
            | Self::DropBatchSize(_)
            | Self::MemoryLimit(_) => 4,
            Self::CeThresholdSelector(_) | Self::CeThresholdMask(_) => 1,
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Target(d)
            | Self::Limit(d)
            | Self::Interval(d)
            | Self::Ecn(d)
            | Self::Flows(d)
            | Self::Quantum(d)
            | Self::CeThreshold(d)
            | Self::DropBatchSize(d)
            | Self::MemoryLimit(d) => NativeEndian::write_u32(buffer, *d),
            Self::CeThresholdSelector(d) | Self::CeThresholdMask(d) => {
                buffer[0] = *d
            }
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Target(_) => TCA_FQ_CODEL_TARGET,
            Self::Limit(_) => TCA_FQ_CODEL_LIMIT,
            Self::Interval(_) => TCA_FQ_CODEL_INTERVAL,
            Self::Ecn(_) => TCA_FQ_CODEL_ECN,
            Self::Flows(_) => TCA_FQ_CODEL_FLOWS,
            Self::Quantum(_) => TCA_FQ_CODEL_QUANTUM,
            Self::CeThreshold(_) => TCA_FQ_CODEL_CE_THRESHOLD,
            Self::DropBatchSize(_) => TCA_FQ_CODEL_DROP_BATCH_SIZE,
            Self::MemoryLimit(_) => TCA_FQ_CODEL_MEMORY_LIMIT,
            Self::CeThresholdSelector(_) => TCA_FQ_CODEL_CE_THRESHOLD_SELECTOR,
            Self::CeThresholdMask(_) => TCA_FQ_CODEL_CE_THRESHOLD_MASK,
            Self::Other(attr) => attr.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for TcQdiscFqCodelOption
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            TCA_FQ_CODEL_TARGET => Self::Target(
                parse_u32(payload)
                    .context("failed to parse TCA_FQ_CODEL_TARGET")?,
            ),
            TCA_FQ_CODEL_LIMIT => Self::Limit(
                parse_u32(payload)
                    .context("failed to parse TCA_FQ_CODEL_LIMIT")?,
            ),
            TCA_FQ_CODEL_INTERVAL => Self::Interval(
                parse_u32(payload)
                    .context("failed to parse TCA_FQ_CODEL_INTERVAL")?,
            ),
            TCA_FQ_CODEL_ECN => Self::Ecn(
                parse_u32(payload)
                    .context("failed to parse TCA_FQ_CODEL_ECN")?,
            ),
            TCA_FQ_CODEL_FLOWS => Self::Flows(
                parse_u32(payload)
                    .context("failed to parse TCA_FQ_CODEL_FLOWS")?,
            ),
            TCA_FQ_CODEL_QUANTUM => Self::Quantum(
                parse_u32(payload)
                    .context("failed to parse TCA_FQ_CODEL_QUANTUM")?,
            ),
            TCA_FQ_CODEL_CE_THRESHOLD => Self::CeThreshold(
                parse_u32(payload)
                    .context("failed to parse TCA_FQ_CODEL_CETHRESHOLD")?,
            ),
            TCA_FQ_CODEL_DROP_BATCH_SIZE => Self::DropBatchSize(
                parse_u32(payload)
                    .context("failed to parse TCA_FQ_CODEL_DROP_BATCH_SIZE")?,
            ),
            TCA_FQ_CODEL_MEMORY_LIMIT => Self::MemoryLimit(
                parse_u32(payload)
                    .context("failed to parse TCA_FQ_CODEL_MEMORY_LIMIT")?,
            ),
            TCA_FQ_CODEL_CE_THRESHOLD_SELECTOR => {
                Self::CeThresholdSelector(parse_u8(payload).context(
                    "failed to parse TCA_FQ_CODEL_CE_THRESHOLD_SELECTOR",
                )?)
            }
            TCA_FQ_CODEL_CE_THRESHOLD_MASK => {
                Self::CeThresholdMask(parse_u8(payload).context(
                    "failed to parse TCA_FQ_CODEL_CE_THRESHOLD_MASK",
                )?)
            }
            _ => Self::Other(
                DefaultNla::parse(buf).context("failed to parse u32 nla")?,
            ),
        })
    }
}
