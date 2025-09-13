// SPDX-License-Identifier: MIT

use std::net::Ipv6Addr;

use anyhow::Context;
use netlink_packet_utils::{
    nla::{self, DefaultNla, NlaBuffer},
    traits::Parseable,
    DecodeError, Emitable,
};

use super::cache_info::{CacheInfo, CacheInfoBuffer};

const PREFIX_ADDRESS: u16 = 1;
const PREFIX_CACHEINFO: u16 = 2;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PrefixAttribute {
    Address(Ipv6Addr),
    CacheInfo(CacheInfo),
    Other(DefaultNla),
}

impl nla::Nla for PrefixAttribute {
    fn value_len(&self) -> usize {
        match *self {
            Self::Address(_) => 16,
            Self::CacheInfo(ref cache_info) => cache_info.buffer_len(),
            Self::Other(ref attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match *self {
            Self::Address(ref addr) => buffer.copy_from_slice(&addr.octets()),
            Self::CacheInfo(ref cache_info) => cache_info.emit(buffer),
            Self::Other(ref attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match *self {
            Self::Address(_) => PREFIX_ADDRESS,
            Self::CacheInfo(_) => PREFIX_CACHEINFO,
            Self::Other(ref nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for PrefixAttribute
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        match buf.kind() {
            PREFIX_ADDRESS => {
                if let Ok(payload) = TryInto::<[u8; 16]>::try_into(payload) {
                    Ok(Self::Address(Ipv6Addr::from(payload)))
                } else {
                    Err(DecodeError::from(format!("Invalid PREFIX_ADDRESS, unexpected payload length: {:?}", payload)))
                }
            }
            PREFIX_CACHEINFO => Ok(Self::CacheInfo(
                CacheInfo::parse(&CacheInfoBuffer::new(payload)).context(
                    format!("Invalid PREFIX_CACHEINFO: {:?}", payload),
                )?,
            )),
            _ => Ok(Self::Other(DefaultNla::parse(buf)?)),
        }
    }
}
