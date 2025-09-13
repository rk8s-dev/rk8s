// SPDX-License-Identifier: MIT

/// Mirror action
///
/// The mirred action allows packet mirroring (copying) or
/// redirecting (stealing) the packet it receives. Mirroring is what
/// is sometimes referred to as Switch Port Analyzer (SPAN) and is
/// commonly used to analyze and/or debug flows.
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    traits::{Emitable, Parseable},
    DecodeError,
};

use super::{
    TcActionGeneric, TcActionGenericBuffer, Tcf, TcfBuffer, TC_TCF_BUF_LEN,
};

/// Traffic control action used to mirror or redirect packets.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct TcActionMirror {}
impl TcActionMirror {
    /// The `TcActionAttribute::Kind` of this action.
    pub const KIND: &'static str = "mirred";
}

const TCA_MIRRED_TM: u16 = 1;
const TCA_MIRRED_PARMS: u16 = 2;

/// Options for the `TcActionMirror` action.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcActionMirrorOption {
    /// Rule installation and usage time
    Tm(Tcf),
    /// Parameters for the mirred action.
    Parms(TcMirror),
    /// Other attributes unknown at the time of writing.
    Other(DefaultNla),
}

impl Nla for TcActionMirrorOption {
    fn value_len(&self) -> usize {
        match self {
            Self::Tm(_) => TC_TCF_BUF_LEN,
            Self::Parms(_) => TC_MIRRED_BUF_LEN,
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
            Self::Tm(_) => TCA_MIRRED_TM,
            Self::Parms(_) => TCA_MIRRED_PARMS,
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for TcActionMirrorOption
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            TCA_MIRRED_TM => {
                Self::Tm(Tcf::parse(&TcfBuffer::new_checked(payload)?)?)
            }
            TCA_MIRRED_PARMS => Self::Parms(TcMirror::parse(
                &TcMirrorBuffer::new_checked(payload)?,
            )?),
            _ => Self::Other(DefaultNla::parse(buf)?),
        })
    }
}

const TC_MIRRED_BUF_LEN: usize = TcActionGeneric::BUF_LEN + 8;

/// Parameters for the mirred action.
#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct TcMirror {
    /// Generic action parameters.
    pub generic: TcActionGeneric,
    /// Describes how the packet be mirrored or redirected.
    pub eaction: TcMirrorActionType,
    /// Interface index to mirror or redirect to.
    pub ifindex: u32,
}

// kernel struct `tc_mirred`
buffer!(TcMirrorBuffer(TC_MIRRED_BUF_LEN) {
    generic: (slice, 0..20),
    eaction: (i32, 20..24),
    ifindex: (u32, 24..28),
});

impl Emitable for TcMirror {
    fn buffer_len(&self) -> usize {
        TC_MIRRED_BUF_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut packet = TcMirrorBuffer::new(buffer);
        self.generic.emit(packet.generic_mut());
        packet.set_eaction(self.eaction.into());
        packet.set_ifindex(self.ifindex);
    }
}

impl<T: AsRef<[u8]> + ?Sized> Parseable<TcMirrorBuffer<&T>> for TcMirror {
    fn parse(buf: &TcMirrorBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self {
            generic: TcActionGeneric::parse(&TcActionGenericBuffer::new(
                buf.generic(),
            ))?,
            eaction: buf.eaction().into(),
            ifindex: buf.ifindex(),
        })
    }
}

const TCA_EGRESS_REDIR: i32 = 1;
const TCA_EGRESS_MIRROR: i32 = 2;
const TCA_INGRESS_REDIR: i32 = 3;
const TCA_INGRESS_MIRROR: i32 = 4;

/// Type of mirroring or redirecting action.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
#[non_exhaustive]
pub enum TcMirrorActionType {
    #[default]
    /// Redirect to the egress pipeline.
    EgressRedir,
    /// Mirror to the egress pipeline.
    EgressMirror,
    /// Redirect to the ingress pipeline.
    IngressRedir,
    /// Mirror to the ingress pipeline.
    IngressMirror,
    /// Other action type unknown at the time of writing.
    Other(i32),
}

impl From<i32> for TcMirrorActionType {
    fn from(d: i32) -> Self {
        match d {
            TCA_EGRESS_REDIR => Self::EgressRedir,
            TCA_EGRESS_MIRROR => Self::EgressMirror,
            TCA_INGRESS_REDIR => Self::IngressRedir,
            TCA_INGRESS_MIRROR => Self::IngressMirror,
            _ => Self::Other(d),
        }
    }
}

impl From<TcMirrorActionType> for i32 {
    fn from(v: TcMirrorActionType) -> i32 {
        match v {
            TcMirrorActionType::EgressRedir => TCA_EGRESS_REDIR,
            TcMirrorActionType::EgressMirror => TCA_EGRESS_MIRROR,
            TcMirrorActionType::IngressRedir => TCA_INGRESS_REDIR,
            TcMirrorActionType::IngressMirror => TCA_INGRESS_MIRROR,
            TcMirrorActionType::Other(d) => d,
        }
    }
}
