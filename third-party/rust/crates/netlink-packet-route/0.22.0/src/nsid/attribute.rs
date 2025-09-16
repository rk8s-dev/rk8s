// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};

use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_i32, parse_u32},
    traits::Parseable,
    DecodeError,
};

const NETNSA_NSID: u16 = 1;
const NETNSA_PID: u16 = 2;
const NETNSA_FD: u16 = 3;
const NETNSA_TARGET_NSID: u16 = 4;
const NETNSA_CURRENT_NSID: u16 = 5;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum NsidAttribute {
    /// -1 means not assigned
    Id(i32),
    Pid(u32),
    Fd(u32),
    TargetNsid(i32),
    CurrentNsid(i32),
    Other(DefaultNla),
}

impl Nla for NsidAttribute {
    fn value_len(&self) -> usize {
        match self {
            Self::Id(_)
            | Self::Pid(_)
            | Self::Fd(_)
            | Self::TargetNsid(_)
            | Self::CurrentNsid(_) => 4,
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Fd(v) | Self::Pid(v) => NativeEndian::write_u32(buffer, *v),
            Self::Id(v) | Self::TargetNsid(v) | Self::CurrentNsid(v) => {
                NativeEndian::write_i32(buffer, *v)
            }
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Id(_) => NETNSA_NSID,
            Self::Pid(_) => NETNSA_PID,
            Self::Fd(_) => NETNSA_FD,
            Self::TargetNsid(_) => NETNSA_TARGET_NSID,
            Self::CurrentNsid(_) => NETNSA_CURRENT_NSID,
            Self::Other(attr) => attr.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for NsidAttribute
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            NETNSA_NSID => {
                Self::Id(parse_i32(payload).context("invalid NETNSA_NSID")?)
            }
            NETNSA_PID => {
                Self::Pid(parse_u32(payload).context("invalid NETNSA_PID")?)
            }
            NETNSA_FD => {
                Self::Fd(parse_u32(payload).context("invalid NETNSA_FD")?)
            }
            NETNSA_TARGET_NSID => Self::TargetNsid(
                parse_i32(payload).context("invalid NETNSA_TARGET_NSID")?,
            ),
            NETNSA_CURRENT_NSID => Self::CurrentNsid(
                parse_i32(payload).context("invalid NETNSA_CURRENT_NSID")?,
            ),
            kind => Self::Other(
                DefaultNla::parse(buf)
                    .context(format!("unknown NLA type {kind}"))?,
            ),
        })
    }
}
