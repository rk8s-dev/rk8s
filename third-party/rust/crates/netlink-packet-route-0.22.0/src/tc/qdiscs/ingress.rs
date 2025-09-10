// SPDX-License-Identifier: MIT

// Currently, the qdisc ingress does not have any attribute, kernel
// just start a empty nla_nest. This is just a place holder

use anyhow::Context;
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    DecodeError, Parseable,
};

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct TcQdiscIngress {}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcQdiscIngressOption {
    Other(DefaultNla),
}

impl TcQdiscIngress {
    pub(crate) const KIND: &'static str = "ingress";
}

impl Nla for TcQdiscIngressOption {
    fn value_len(&self) -> usize {
        match self {
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Other(attr) => attr.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for TcQdiscIngressOption
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        Ok(Self::Other(
            DefaultNla::parse(buf).context("failed to parse ingress nla")?,
        ))
    }
}
