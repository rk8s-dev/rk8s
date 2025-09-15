// SPDX-License-Identifier: MIT

use std::{mem::size_of, os::fd::RawFd};

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    parsers::{parse_i32, parse_u32, parse_u8},
    DecodeError, Parseable,
};

const IFLA_XDP_FD: u32 = 1;
const IFLA_XDP_ATTACHED: u32 = 2;
const IFLA_XDP_FLAGS: u32 = 3;
const IFLA_XDP_PROG_ID: u32 = 4;
const IFLA_XDP_DRV_PROG_ID: u32 = 5;
const IFLA_XDP_SKB_PROG_ID: u32 = 6;
const IFLA_XDP_HW_PROG_ID: u32 = 7;
const IFLA_XDP_EXPECTED_FD: u32 = 8;

const XDP_ATTACHED_NONE: u8 = 0;
const XDP_ATTACHED_DRV: u8 = 1;
const XDP_ATTACHED_SKB: u8 = 2;
const XDP_ATTACHED_HW: u8 = 3;
const XDP_ATTACHED_MULTI: u8 = 4;

#[non_exhaustive]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum LinkXdp {
    Fd(RawFd),
    Attached(XdpAttached),
    Flags(u32),
    ProgId(u32),
    DrvProgId(u32),
    SkbProgId(u32),
    HwProgId(u32),
    ExpectedFd(u32),
    Other(DefaultNla),
}

impl Nla for LinkXdp {
    fn value_len(&self) -> usize {
        match self {
            Self::Fd(_) => size_of::<RawFd>(),
            Self::Attached(_) => size_of::<u8>(),
            Self::Flags(_) => size_of::<u32>(),
            Self::ProgId(_) => size_of::<u32>(),
            Self::DrvProgId(_) => size_of::<u32>(),
            Self::SkbProgId(_) => size_of::<u32>(),
            Self::HwProgId(_) => size_of::<u32>(),
            Self::ExpectedFd(_) => size_of::<u32>(),
            Self::Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Fd(ref value) => NativeEndian::write_i32(buffer, *value),
            Self::Attached(ref value) => buffer[0] = value.as_u8(),
            Self::Flags(ref value) => NativeEndian::write_u32(buffer, *value),
            Self::ProgId(ref value) => NativeEndian::write_u32(buffer, *value),
            Self::DrvProgId(ref value) => {
                NativeEndian::write_u32(buffer, *value)
            }
            Self::SkbProgId(ref value) => {
                NativeEndian::write_u32(buffer, *value)
            }
            Self::HwProgId(ref value) => {
                NativeEndian::write_u32(buffer, *value)
            }
            Self::ExpectedFd(ref value) => {
                NativeEndian::write_u32(buffer, *value)
            }
            Self::Other(ref nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Fd(_) => IFLA_XDP_FD as u16,
            Self::Attached(_) => IFLA_XDP_ATTACHED as u16,
            Self::Flags(_) => IFLA_XDP_FLAGS as u16,
            Self::ProgId(_) => IFLA_XDP_PROG_ID as u16,
            Self::DrvProgId(_) => IFLA_XDP_DRV_PROG_ID as u16,
            Self::SkbProgId(_) => IFLA_XDP_SKB_PROG_ID as u16,
            Self::HwProgId(_) => IFLA_XDP_HW_PROG_ID as u16,
            Self::ExpectedFd(_) => IFLA_XDP_EXPECTED_FD as u16,
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for LinkXdp {
    fn parse(nla: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = nla.value();
        Ok(match nla.kind() as u32 {
            IFLA_XDP_FD => Self::Fd(
                parse_i32(payload).context("invalid IFLA_XDP_FD value")?,
            ),
            IFLA_XDP_ATTACHED => {
                let err = "invalid IFLA_XDP_ATTACHED value";
                let value = parse_u8(payload).context(err)?;
                Self::Attached(XdpAttached::try_from(value).context(err)?)
            }
            IFLA_XDP_FLAGS => Self::Flags(
                parse_u32(payload).context("invalid IFLA_XDP_FLAGS value")?,
            ),
            IFLA_XDP_PROG_ID => Self::ProgId(
                parse_u32(payload).context("invalid IFLA_XDP_PROG_ID value")?,
            ),
            IFLA_XDP_DRV_PROG_ID => Self::DrvProgId(
                parse_u32(payload).context("invalid IFLA_XDP_PROG_ID value")?,
            ),
            IFLA_XDP_SKB_PROG_ID => Self::SkbProgId(
                parse_u32(payload).context("invalid IFLA_XDP_PROG_ID value")?,
            ),
            IFLA_XDP_HW_PROG_ID => Self::HwProgId(
                parse_u32(payload).context("invalid IFLA_XDP_PROG_ID value")?,
            ),
            IFLA_XDP_EXPECTED_FD => Self::ExpectedFd(
                parse_u32(payload).context("invalid IFLA_XDP_PROG_ID value")?,
            ),
            _ => Self::Other(
                DefaultNla::parse(nla)
                    .context(format!("unknown NLA type {}", nla.kind()))?,
            ),
        })
    }
}

pub(crate) struct VecLinkXdp(pub(crate) Vec<LinkXdp>);

// These NLAs are nested, meaning they are NLAs that contain NLAs. These NLAs
// can contain more nested NLAs nla->type     // IFLA_XDP
// nla->len
// nla->data[]   // <- You are here == Vec<Xdp>
//  nla->data[0].type   <- nla.kind()
//  nla->data[0].len
impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for VecLinkXdp {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut res = Vec::new();
        let nlas = NlasIterator::new(buf.into_inner());
        for nla in nlas {
            let nla = nla?;
            res.push(LinkXdp::parse(&nla)?);
        }
        Ok(VecLinkXdp(res))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum XdpAttached {
    /// XDP_ATTACHED_NONE
    None,
    /// XDP_ATTACHED_DRV
    Driver,
    /// XDP_ATTACHED_SKB
    SocketBuffer,
    /// XDP_ATTACHED_HW
    Hardware,
    /// XDP_ATTACHED_MULTI
    Multiple,
    /// This crate is unaware of the attachment type the kernel is reporting
    Other(u8),
}

impl TryFrom<u8> for XdpAttached {
    type Error = DecodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            XDP_ATTACHED_NONE => Ok(XdpAttached::None),
            XDP_ATTACHED_DRV => Ok(XdpAttached::Driver),
            XDP_ATTACHED_SKB => Ok(XdpAttached::SocketBuffer),
            XDP_ATTACHED_HW => Ok(XdpAttached::Hardware),
            XDP_ATTACHED_MULTI => Ok(XdpAttached::Multiple),
            _ => Ok(XdpAttached::Other(value)),
        }
    }
}

impl XdpAttached {
    fn as_u8(&self) -> u8 {
        match self {
            XdpAttached::None => XDP_ATTACHED_NONE,
            XdpAttached::Driver => XDP_ATTACHED_DRV,
            XdpAttached::SocketBuffer => XDP_ATTACHED_SKB,
            XdpAttached::Hardware => XDP_ATTACHED_HW,
            XdpAttached::Multiple => XDP_ATTACHED_MULTI,
            XdpAttached::Other(other) => *other,
        }
    }
}
