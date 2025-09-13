// SPDX-License-Identifier: MIT

/// Matchall filter
///
/// Matches all packets and performs an action on them.
use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    parsers::parse_u32,
    traits::{Emitable, Parseable},
    DecodeError,
};

use crate::tc::{TcAction, TcHandle};

const TCA_MATCHALL_CLASSID: u16 = 1;
const TCA_MATCHALL_ACT: u16 = 2;
const TCA_MATCHALL_FLAGS: u16 = 3;
const TCA_MATCHALL_PCNT: u16 = 4;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct TcFilterMatchAll {}
impl TcFilterMatchAll {
    pub const KIND: &'static str = "matchall";
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcFilterMatchAllOption {
    ClassId(TcHandle),
    Action(Vec<TcAction>),
    Pnct(Vec<u8>),
    Flags(u32),
    Other(DefaultNla),
}

impl Nla for TcFilterMatchAllOption {
    fn value_len(&self) -> usize {
        match self {
            Self::Pnct(b) => b.len(),
            Self::ClassId(_) => 4,
            Self::Flags(_) => 4,
            Self::Action(acts) => acts.as_slice().buffer_len(),
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Pnct(b) => buffer.copy_from_slice(b.as_slice()),
            Self::ClassId(i) => NativeEndian::write_u32(buffer, (*i).into()),
            Self::Flags(i) => NativeEndian::write_u32(buffer, *i),
            Self::Action(acts) => acts.as_slice().emit(buffer),
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::ClassId(_) => TCA_MATCHALL_CLASSID,
            Self::Action(_) => TCA_MATCHALL_ACT,
            Self::Pnct(_) => TCA_MATCHALL_PCNT,
            Self::Flags(_) => TCA_MATCHALL_FLAGS,
            Self::Other(attr) => attr.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for TcFilterMatchAllOption
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            TCA_MATCHALL_CLASSID => Self::ClassId(
                parse_u32(payload)
                    .context("failed to parse TCA_MATCHALL_UNSPEC")?
                    .into(),
            ),
            TCA_MATCHALL_ACT => {
                let mut acts = vec![];
                for act in NlasIterator::new(payload) {
                    let act = act.context("invalid TCA_MATCHALL_ACT")?;
                    acts.push(
                        TcAction::parse(&act)
                            .context("failed to parse TCA_MATCHALL_ACT")?,
                    );
                }
                Self::Action(acts)
            }
            TCA_MATCHALL_PCNT => Self::Pnct(payload.to_vec()),
            TCA_MATCHALL_FLAGS => Self::Flags(
                parse_u32(payload)
                    .context("failed to parse TCA_MATCHALL_FLAGS")?,
            ),
            _ => Self::Other(
                DefaultNla::parse(buf).context("failed to parse u32 nla")?,
            ),
        })
    }
}
