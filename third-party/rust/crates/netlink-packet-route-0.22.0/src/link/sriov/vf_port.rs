// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    DecodeError, Emitable, Parseable,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VecLinkVfPort(pub(crate) Vec<LinkVfPort>);

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for VecLinkVfPort
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut nlas = vec![];
        for nla in NlasIterator::new(buf.into_inner()) {
            let nla = &nla.context(format!(
                "invalid IFLA_VF_PORTS value: {:?}",
                buf.value()
            ))?;
            if nla.kind() == IFLA_VF_PORT {
                nlas.push(LinkVfPort::parse(&NlaBuffer::new_checked(
                    nla.value(),
                )?)?);
            } else {
                log::warn!(
                    "BUG: Expecting IFLA_VF_PORT in IFLA_VF_PORTS, \
                    but got {}",
                    nla.kind()
                );
            }
        }
        Ok(Self(nlas))
    }
}

const IFLA_VF_PORT: u16 = 1;

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct LinkVfPort(pub Vec<VfPort>);

impl Nla for LinkVfPort {
    fn value_len(&self) -> usize {
        self.0.as_slice().buffer_len()
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        self.0.as_slice().emit(buffer)
    }

    fn kind(&self) -> u16 {
        IFLA_VF_PORT
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for LinkVfPort {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut nlas = vec![];
        for nla in NlasIterator::new(buf.into_inner()) {
            let nla = &nla.context(format!(
                "invalid IFLA_VF_PORT value {:?}",
                buf.value()
            ))?;
            nlas.push(VfPort::parse(nla)?);
        }
        Ok(Self(nlas))
    }
}

/*
const IFLA_PORT_VF: u16 = 1;
const IFLA_PORT_PROFILE: u16 = 2;
// No kernel code is accepting or generating IFLA_PORT_VSI_TYPE.
// const IFLA_PORT_VSI_TYPE: u16 = 3;
const IFLA_PORT_INSTANCE_UUID: u16 = 4;
const IFLA_PORT_HOST_UUID: u16 = 5;
const IFLA_PORT_REQUEST: u16 = 6;
const IFLA_PORT_RESPONSE: u16 = 7;

const UUID_LEN: usize = 16;
*/

#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum VfPort {
    //    Vf(u32),
    //    Profile(String),
    //    InstanceUuid([u8; UUID_LEN]),
    //    HostUuid([u8; UUID_LEN]),
    //    Request(u8),
    Other(DefaultNla),
}

impl Nla for VfPort {
    fn value_len(&self) -> usize {
        match self {
            Self::Other(v) => v.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Other(v) => v.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for VfPort {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        #[allow(clippy::match_single_binding)]
        Ok(match buf.kind() {
            kind => Self::Other(DefaultNla::parse(buf).context(format!(
                "failed to parse {kind} as DefaultNla: {payload:?}"
            ))?),
        })
    }
}
