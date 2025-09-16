// SPDX-License-Identifier: MIT

/// Nat action
///
/// The nat action maps one IP prefix to another
use std::net::Ipv4Addr;

use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    traits::{Emitable, Parseable},
    DecodeError,
};

use super::{
    nat_flag::TcNatFlags, TcActionGeneric, TcActionGenericBuffer, Tcf,
    TcfBuffer, TC_TCF_BUF_LEN,
};

const TCA_NAT_PARMS: u16 = 1;
const TCA_NAT_TM: u16 = 2;

/// Network address translation action.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct TcActionNat {}

impl TcActionNat {
    pub(crate) const KIND: &'static str = "nat";
}

/// Options for the [`TcActionNat`] action.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcActionNatOption {
    /// Rule installation and usage time
    Tm(Tcf),
    /// Parameters for the nat action.
    Parms(TcNat),
    /// Other attributes unknown at the time of writing.
    Other(DefaultNla),
}

impl Nla for TcActionNatOption {
    fn value_len(&self) -> usize {
        match self {
            Self::Tm(_) => TC_TCF_BUF_LEN,
            Self::Parms(v) => v.buffer_len(),
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Tm(p) => p.emit(buffer),
            Self::Parms(p) => p.emit(buffer),
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }
    fn kind(&self) -> u16 {
        match self {
            Self::Tm(_) => TCA_NAT_TM,
            Self::Parms(_) => TCA_NAT_PARMS,
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for TcActionNatOption
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            TCA_NAT_TM => {
                Self::Tm(Tcf::parse(&TcfBuffer::new_checked(payload)?)?)
            }
            TCA_NAT_PARMS => {
                Self::Parms(TcNat::parse(&TcNatBuffer::new_checked(payload)?)?)
            }
            _ => Self::Other(DefaultNla::parse(buf)?),
        })
    }
}

const TC_NAT_BUF_LEN: usize = TcActionGeneric::BUF_LEN + 16;

/// Network address translation action.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct TcNat {
    /// Common attributes for all actions.
    pub generic: TcActionGeneric,
    /// Original address.
    pub old_addr: Ipv4Addr,
    /// New address.
    pub new_addr: Ipv4Addr,
    /// Mask of the old address
    pub mask: Ipv4Addr,
    /// Flags for the NAT action.
    pub flags: TcNatFlags,
}

impl Default for TcNat {
    fn default() -> Self {
        Self {
            generic: TcActionGeneric::default(),
            old_addr: Ipv4Addr::UNSPECIFIED,
            new_addr: Ipv4Addr::UNSPECIFIED,
            mask: Ipv4Addr::UNSPECIFIED,
            flags: TcNatFlags::empty(),
        }
    }
}

buffer!(TcNatBuffer(TC_NAT_BUF_LEN) {
    generic: (slice, 0..TcActionGeneric::BUF_LEN),
    old_addr: (slice, TcActionGeneric::BUF_LEN..(TcActionGeneric::BUF_LEN+4)),
    new_addr: (slice, (TcActionGeneric::BUF_LEN+4)..(TcActionGeneric::BUF_LEN+8)),
    mask: (slice, (TcActionGeneric::BUF_LEN+8)..(TcActionGeneric::BUF_LEN+12)),
    flags: (u32, (TcActionGeneric::BUF_LEN+12)..TC_NAT_BUF_LEN),
});

impl Emitable for TcNat {
    fn buffer_len(&self) -> usize {
        TC_NAT_BUF_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = TcNatBuffer::new(buffer);
        self.generic.emit(packet.generic_mut());
        packet
            .old_addr_mut()
            .copy_from_slice(&self.old_addr.octets());
        packet
            .new_addr_mut()
            .copy_from_slice(&self.new_addr.octets());
        packet.mask_mut().copy_from_slice(&self.mask.octets());
        packet.set_flags(self.flags.bits());
    }
}

impl<T: AsRef<[u8]> + ?Sized> Parseable<TcNatBuffer<&T>> for TcNat {
    fn parse(buf: &TcNatBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self {
            generic: TcActionGeneric::parse(&TcActionGenericBuffer::new(
                buf.generic(),
            ))?,
            old_addr: parse_ipv4(buf.old_addr())?,
            new_addr: parse_ipv4(buf.new_addr())?,
            mask: parse_ipv4(buf.mask())?,
            flags: TcNatFlags::from_bits_retain(buf.flags()),
        })
    }
}

fn parse_ipv4(data: &[u8]) -> Result<Ipv4Addr, DecodeError> {
    if data.len() != 4 {
        Err(DecodeError::from(format!(
            "Invalid length of IPv4 Address, expecting 4 bytes, but got {:?}",
            data
        )))
    } else {
        Ok(Ipv4Addr::new(data[0], data[1], data[2], data[3]))
    }
}
