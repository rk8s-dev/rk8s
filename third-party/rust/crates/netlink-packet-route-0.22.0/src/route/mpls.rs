// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::parse_u8,
    traits::{Emitable, Parseable},
    DecodeError,
};

const MPLS_IPTUNNEL_DST: u16 = 1;
const MPLS_IPTUNNEL_TTL: u16 = 2;

/// Netlink attributes for `RTA_ENCAP` with `RTA_ENCAP_TYPE` set to
/// `LWTUNNEL_ENCAP_MPLS`.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum RouteMplsIpTunnel {
    Destination(Vec<MplsLabel>),
    Ttl(u8),
    Other(DefaultNla),
}

impl Nla for RouteMplsIpTunnel {
    fn value_len(&self) -> usize {
        match self {
            Self::Destination(v) => VecMplsLabel(v.to_vec()).buffer_len(),
            Self::Ttl(_) => 1,
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Destination(_) => MPLS_IPTUNNEL_DST,
            Self::Ttl(_) => MPLS_IPTUNNEL_TTL,
            Self::Other(attr) => attr.kind(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Destination(v) => VecMplsLabel(v.to_vec()).emit(buffer),
            Self::Ttl(ttl) => buffer[0] = *ttl,
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for RouteMplsIpTunnel
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            MPLS_IPTUNNEL_DST => Self::Destination(
                VecMplsLabel::parse(payload)
                    .context(format!(
                        "invalid MPLS_IPTUNNEL_DST value {:?}",
                        payload
                    ))?
                    .0,
            ),
            MPLS_IPTUNNEL_TTL => Self::Ttl(
                parse_u8(payload).context("invalid MPLS_IPTUNNEL_TTL value")?,
            ),
            _ => Self::Other(
                DefaultNla::parse(buf)
                    .context("invalid NLA value (unknown type) value")?,
            ),
        })
    }
}

const MPLS_LS_LABEL_MASK: u32 = 0xFFFFF000;
const MPLS_LS_LABEL_SHIFT: u32 = 12;
const MPLS_LS_TC_MASK: u32 = 0x00000E00;
const MPLS_LS_TC_SHIFT: u32 = 9;
const MPLS_LS_S_MASK: u32 = 0x00000100;
const MPLS_LS_S_SHIFT: u32 = 8;
const MPLS_LS_TTL_MASK: u32 = 0x000000FF;
const MPLS_LS_TTL_SHIFT: u32 = 0;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
/// MPLS label defined in RFC 3032 and updated by RFC 5462
pub struct MplsLabel {
    /// label, 20 bytes
    pub label: u32,
    /// Traffic Class, 3 bits
    pub traffic_class: u8,
    /// Bottom of Stack, 1 bit
    pub bottom_of_stack: bool,
    /// Time to Live
    pub ttl: u8,
}

impl MplsLabel {
    pub(crate) fn parse(payload: &[u8]) -> Result<Self, DecodeError> {
        if payload.len() == 4 {
            Ok(Self::from(u32::from_be_bytes([
                payload[0], payload[1], payload[2], payload[3],
            ])))
        } else {
            Err(DecodeError::from(format!(
                "Invalid u8 array length {}, expecting \
                4 bytes for MPLS label, got {:?}",
                payload.len(),
                payload,
            )))
        }
    }
}

impl Emitable for MplsLabel {
    fn buffer_len(&self) -> usize {
        4
    }

    fn emit(&self, buffer: &mut [u8]) {
        buffer.copy_from_slice(u32::from(*self).to_be_bytes().as_slice())
    }
}

impl From<u32> for MplsLabel {
    fn from(d: u32) -> Self {
        let label = (d & MPLS_LS_LABEL_MASK) >> MPLS_LS_LABEL_SHIFT;
        let traffic_class = ((d & MPLS_LS_TC_MASK) >> MPLS_LS_TC_SHIFT) as u8;
        let bottom_of_stack = (d & MPLS_LS_S_MASK) > 0;
        let ttl = (d & MPLS_LS_TTL_MASK) as u8;
        Self {
            label,
            traffic_class,
            bottom_of_stack,
            ttl,
        }
    }
}

impl From<MplsLabel> for u32 {
    fn from(v: MplsLabel) -> u32 {
        (v.label << MPLS_LS_LABEL_SHIFT)
            | ((v.traffic_class as u32) << MPLS_LS_TC_SHIFT)
            | ((v.bottom_of_stack as u32) << MPLS_LS_S_SHIFT)
            | ((v.ttl as u32) << MPLS_LS_TTL_SHIFT)
    }
}

pub(crate) struct VecMplsLabel(pub(crate) Vec<MplsLabel>);

impl VecMplsLabel {
    pub(crate) fn parse(payload: &[u8]) -> Result<Self, DecodeError> {
        let mut labels = vec![];
        let mut i: usize = 0;
        while i + 4 <= payload.len() {
            labels.push(MplsLabel::parse(&payload[i..i + 4])?);
            i += 4;
        }
        Ok(Self(labels))
    }
}

impl Emitable for VecMplsLabel {
    fn buffer_len(&self) -> usize {
        self.0.len() * 4
    }

    fn emit(&self, buffer: &mut [u8]) {
        for (i, label) in self.0.iter().enumerate() {
            label.emit(&mut buffer[i * 4..i * 4 + 4]);
        }
    }
}

const MPLS_TTL_PROP_DEFAULT: u8 = 0;
const MPLS_TTL_PROP_ENABLED: u8 = 1;
const MPLS_TTL_PROP_DISABLED: u8 = 2;

#[derive(Debug, PartialEq, Eq, Clone, Default, Copy)]
#[non_exhaustive]
pub enum RouteMplsTtlPropagation {
    #[default]
    Default,
    Enabled,
    Disabled,
    Other(u8),
}

impl From<u8> for RouteMplsTtlPropagation {
    fn from(d: u8) -> Self {
        match d {
            MPLS_TTL_PROP_DEFAULT => Self::Default,
            MPLS_TTL_PROP_ENABLED => Self::Enabled,
            MPLS_TTL_PROP_DISABLED => Self::Disabled,
            _ => Self::Other(d),
        }
    }
}

impl From<RouteMplsTtlPropagation> for u8 {
    fn from(v: RouteMplsTtlPropagation) -> u8 {
        match v {
            RouteMplsTtlPropagation::Default => MPLS_TTL_PROP_DEFAULT,
            RouteMplsTtlPropagation::Enabled => MPLS_TTL_PROP_ENABLED,
            RouteMplsTtlPropagation::Disabled => MPLS_TTL_PROP_DISABLED,
            RouteMplsTtlPropagation::Other(d) => d,
        }
    }
}
