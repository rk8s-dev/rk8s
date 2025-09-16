// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    traits::Parseable,
    DecodeError,
};

#[derive(Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
pub enum LinkProtoInfoInet6 {
    Other(DefaultNla),
}

pub(crate) struct VecLinkProtoInfoInet6(pub(crate) Vec<LinkProtoInfoInet6>);

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for VecLinkProtoInfoInet6
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut nlas = vec![];
        for nla in NlasIterator::new(buf.into_inner()) {
            let nla = nla.context(format!(
                "invalid inet6 IFLA_PROTINFO {:?}",
                buf.value()
            ))?;
            nlas.push(LinkProtoInfoInet6::parse(&nla)?);
        }
        Ok(Self(nlas))
    }
}

impl Nla for LinkProtoInfoInet6 {
    fn value_len(&self) -> usize {
        match *self {
            Self::Other(ref nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match *self {
            Self::Other(ref nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match *self {
            Self::Other(ref nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for LinkProtoInfoInet6
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        Ok(Self::Other(DefaultNla::parse(buf).context(format!(
            "invalid inet6 IFLA_PROTINFO {:?}",
            buf.value()
        ))?))
    }
}
