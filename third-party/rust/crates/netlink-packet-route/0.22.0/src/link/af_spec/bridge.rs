// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    parsers::{parse_u16, parse_u32},
    traits::Parseable,
    DecodeError, Emitable,
};

const IFLA_BRIDGE_FLAGS: u16 = 0;
const IFLA_BRIDGE_MODE: u16 = 1;
const IFLA_BRIDGE_VLAN_INFO: u16 = 2;
const IFLA_BRIDGE_VLAN_TUNNEL_INFO: u16 = 3;
// const IFLA_BRIDGE_MRP: u16 = 4;
// const IFLA_BRIDGE_CFM: u16 = 5;
// const IFLA_BRIDGE_MST: u16 = 6;

#[derive(Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
pub enum AfSpecBridge {
    Flags(BridgeFlag),
    Mode(BridgeMode),
    VlanInfo(BridgeVlanInfo),
    VlanTunnelInfo(Vec<BridgeVlanTunnelInfo>),
    Other(DefaultNla),
}

impl Nla for AfSpecBridge {
    fn value_len(&self) -> usize {
        match self {
            Self::Flags(_) => BridgeFlag::LENGTH,
            Self::Mode(_) => BridgeMode::LENGTH,
            Self::VlanInfo(_) => BridgeVlanInfo::LENGTH,
            Self::VlanTunnelInfo(s) => s.as_slice().buffer_len(),
            Self::Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Flags(value) => {
                NativeEndian::write_u16(buffer, u16::from(*value))
            }
            Self::Mode(value) => {
                NativeEndian::write_u16(buffer, u16::from(*value))
            }
            Self::VlanInfo(info) => {
                buffer[..4].copy_from_slice(<[u8; 4]>::from(info).as_slice())
            }
            Self::VlanTunnelInfo(s) => s.as_slice().emit(buffer),
            Self::Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Flags(_) => IFLA_BRIDGE_FLAGS,
            Self::Mode(_) => IFLA_BRIDGE_MODE,
            Self::VlanInfo(_) => IFLA_BRIDGE_VLAN_INFO,
            Self::VlanTunnelInfo(_) => IFLA_BRIDGE_VLAN_TUNNEL_INFO,
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for AfSpecBridge {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_BRIDGE_FLAGS => Self::Flags(
                parse_u16(payload)
                    .context("Invalid IFLA_BRIDGE_FLAGS value")?
                    .into(),
            ),
            IFLA_BRIDGE_MODE => Self::Mode(
                parse_u16(payload)
                    .context("Invalid IFLA_BRIDGE_MODE value")?
                    .into(),
            ),
            IFLA_BRIDGE_VLAN_INFO => Self::VlanInfo(
                BridgeVlanInfo::try_from(payload)
                    .context("Invalid IFLA_BRIDGE_VLAN_INFO value")?,
            ),
            IFLA_BRIDGE_VLAN_TUNNEL_INFO => {
                let mut nlas = Vec::new();
                for nla in NlasIterator::new(payload) {
                    let nla = &nla.context(format!(
                        "Invalid IFLA_BRIDGE_VLAN_TUNNEL_INFO for {payload:?}"
                    ))?;
                    let parsed = BridgeVlanTunnelInfo::parse(nla)?;
                    nlas.push(parsed);
                }
                Self::VlanTunnelInfo(nlas)
            }
            kind => Self::Other(
                DefaultNla::parse(buf)
                    .context(format!("Unknown NLA type {kind}"))?,
            ),
        })
    }
}

#[cfg(any(target_os = "linux", target_os = "fuchsia"))]
pub(crate) struct VecAfSpecBridge(pub(crate) Vec<AfSpecBridge>);

#[cfg(any(target_os = "linux", target_os = "fuchsia"))]
impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for VecAfSpecBridge
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut nlas = vec![];
        let err = "Invalid AF_INET NLA for IFLA_AF_SPEC(AF_BRIDGE)";
        for nla in NlasIterator::new(buf.into_inner()) {
            let nla = nla.context(err)?;
            nlas.push(AfSpecBridge::parse(&nla).context(err)?);
        }
        Ok(Self(nlas))
    }
}

const BRIDGE_VLAN_INFO_CONTROLLER: u16 = 1 << 0;
const BRIDGE_VLAN_INFO_PVID: u16 = 1 << 1;
const BRIDGE_VLAN_INFO_UNTAGGED: u16 = 1 << 2;
const BRIDGE_VLAN_INFO_RANGE_BEGIN: u16 = 1 << 3;
const BRIDGE_VLAN_INFO_RANGE_END: u16 = 1 << 4;
const BRIDGE_VLAN_INFO_BRENTRY: u16 = 1 << 5;
const BRIDGE_VLAN_INFO_ONLY_OPTS: u16 = 1 << 6;

bitflags! {
    #[non_exhaustive]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct BridgeVlanInfoFlags: u16 {
        /// Operate on Bridge device as well
        const Controller = BRIDGE_VLAN_INFO_CONTROLLER;
        /// VLAN is PVID, ingress untagged
        const Pvid = BRIDGE_VLAN_INFO_PVID;
        /// VLAN egresses untagged
        const Untagged= BRIDGE_VLAN_INFO_UNTAGGED;
        /// VLAN is start of vlan range
        const RangeBegin = BRIDGE_VLAN_INFO_RANGE_BEGIN;
        /// VLAN is end of vlan range
        const RangeEnd = BRIDGE_VLAN_INFO_RANGE_END;
        /// Global bridge VLAN entry
        const Brentry = BRIDGE_VLAN_INFO_BRENTRY;
        /// Skip create/delete/flags
        const OnlyOpts= BRIDGE_VLAN_INFO_ONLY_OPTS;
        const _ = !0;
    }
}

impl BridgeVlanInfoFlags {
    pub const LENGTH: usize = 2;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct BridgeVlanInfo {
    pub flags: BridgeVlanInfoFlags,
    pub vid: u16,
}

impl BridgeVlanInfo {
    pub const LENGTH: usize = 4;
}

impl From<&BridgeVlanInfo> for [u8; 4] {
    fn from(d: &BridgeVlanInfo) -> Self {
        let mut ret = [0u8; 4];
        NativeEndian::write_u16(&mut ret[0..2], d.flags.bits());
        NativeEndian::write_u16(&mut ret[2..4], d.vid);
        ret
    }
}

impl TryFrom<&[u8]> for BridgeVlanInfo {
    type Error = DecodeError;
    fn try_from(raw: &[u8]) -> Result<Self, DecodeError> {
        if raw.len() == 4 {
            Ok(Self {
                flags: BridgeVlanInfoFlags::from_bits_retain(
                    parse_u16(&raw[0..2]).context(format!(
                        "Invalid IFLA_BRIDGE_VLAN_INFO value: {raw:?}"
                    ))?,
                ),
                vid: parse_u16(&raw[2..4]).context(format!(
                    "Invalid IFLA_BRIDGE_VLAN_INFO value: {raw:?}"
                ))?,
            })
        } else {
            Err(DecodeError::from(format!(
                "Invalid IFLA_BRIDGE_VLAN_INFO value, expecting [u8;4], \
                but got {raw:?}"
            )))
        }
    }
}

// kernel constant name is BRIDGE_FLAGS_MASTER
const BRIDGE_FLAGS_CONTROLLER: u16 = 1;
const BRIDGE_FLAGS_SELF: u16 = 2;

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[non_exhaustive]
pub enum BridgeFlag {
    /// Bridge command to/from controller
    Controller,
    /// Bridge command to/from lowerdev
    LowerDev,
    Other(u16),
}

impl From<u16> for BridgeFlag {
    fn from(d: u16) -> Self {
        match d {
            BRIDGE_FLAGS_CONTROLLER => Self::Controller,
            BRIDGE_FLAGS_SELF => Self::LowerDev,
            _ => Self::Other(d),
        }
    }
}

impl From<BridgeFlag> for u16 {
    fn from(v: BridgeFlag) -> u16 {
        match v {
            BridgeFlag::Controller => BRIDGE_FLAGS_CONTROLLER,
            BridgeFlag::LowerDev => BRIDGE_FLAGS_SELF,
            BridgeFlag::Other(d) => d,
        }
    }
}

impl BridgeFlag {
    pub const LENGTH: usize = 2;
}

const BRIDGE_MODE_VEB: u16 = 0;
const BRIDGE_MODE_VEPA: u16 = 1;

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[non_exhaustive]
pub enum BridgeMode {
    /// Default loopback mode
    Veb,
    /// 802.1Qbg defined VEPA mode
    Vepa,
    Other(u16),
}

impl From<u16> for BridgeMode {
    fn from(d: u16) -> Self {
        match d {
            BRIDGE_MODE_VEB => Self::Veb,
            BRIDGE_MODE_VEPA => Self::Vepa,
            _ => Self::Other(d),
        }
    }
}

impl From<BridgeMode> for u16 {
    fn from(v: BridgeMode) -> u16 {
        match v {
            BridgeMode::Veb => BRIDGE_MODE_VEB,
            BridgeMode::Vepa => BRIDGE_MODE_VEPA,
            BridgeMode::Other(d) => d,
        }
    }
}

impl BridgeMode {
    pub const LENGTH: usize = 2;
}

const IFLA_BRIDGE_VLAN_TUNNEL_ID: u16 = 1;
const IFLA_BRIDGE_VLAN_TUNNEL_VID: u16 = 2;
const IFLA_BRIDGE_VLAN_TUNNEL_FLAGS: u16 = 3;

#[derive(Clone, Eq, PartialEq, Debug)]
#[non_exhaustive]
pub enum BridgeVlanTunnelInfo {
    Id(u32),
    Vid(u16),
    Flags(BridgeVlanInfoFlags),
    Other(DefaultNla),
}

impl Nla for BridgeVlanTunnelInfo {
    fn value_len(&self) -> usize {
        match self {
            Self::Id(_) => 4,
            Self::Vid(_) => 2,
            Self::Flags(_) => BridgeVlanInfoFlags::LENGTH,
            Self::Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Id(v) => NativeEndian::write_u32(buffer, *v),
            Self::Vid(v) => NativeEndian::write_u16(buffer, *v),
            Self::Flags(value) => NativeEndian::write_u16(buffer, value.bits()),
            Self::Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Id(_) => IFLA_BRIDGE_VLAN_TUNNEL_ID,
            Self::Vid(_) => IFLA_BRIDGE_VLAN_TUNNEL_VID,
            Self::Flags(_) => IFLA_BRIDGE_VLAN_TUNNEL_FLAGS,
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for BridgeVlanTunnelInfo
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_BRIDGE_VLAN_TUNNEL_ID => {
                Self::Id(parse_u32(payload).context(format!(
                    "Invalid IFLA_BRIDGE_VLAN_TUNNEL_ID {payload:?}"
                ))?)
            }
            IFLA_BRIDGE_VLAN_TUNNEL_VID => {
                Self::Vid(parse_u16(payload).context(format!(
                    "Invalid IFLA_BRIDGE_VLAN_TUNNEL_VID {payload:?}"
                ))?)
            }
            IFLA_BRIDGE_VLAN_TUNNEL_FLAGS => {
                Self::Flags(BridgeVlanInfoFlags::from_bits_retain(
                    parse_u16(payload).context(format!(
                        "Invalid IFLA_BRIDGE_VLAN_TUNNEL_VID {payload:?}"
                    ))?,
                ))
            }
            _ => {
                Self::Other(DefaultNla::parse(buf).context("Unknown NLA type")?)
            }
        })
    }
}
