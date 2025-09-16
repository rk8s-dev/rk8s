// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    DecodeError, Emitable, Parseable,
};

use crate::link::{
    VfInfoBroadcast, VfInfoBroadcastBuffer, VfInfoGuid, VfInfoGuidBuffer,
    VfInfoLinkState, VfInfoLinkStateBuffer, VfInfoMac, VfInfoMacBuffer,
    VfInfoRate, VfInfoRateBuffer, VfInfoRssQueryEn, VfInfoRssQueryEnBuffer,
    VfInfoSpoofCheck, VfInfoSpoofCheckBuffer, VfInfoTrust, VfInfoTrustBuffer,
    VfInfoTxRate, VfInfoTxRateBuffer, VfInfoVlan, VfInfoVlanBuffer, VfStats,
    VfVlan,
};

const IFLA_VF_INFO: u16 = 1;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VecLinkVfInfo(pub(crate) Vec<LinkVfInfo>);

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for VecLinkVfInfo
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut nlas = vec![];
        for nla in NlasIterator::new(buf.into_inner()) {
            let nla = &nla.context(format!(
                "invalid IFLA_VFINFO_LIST value: {:?}",
                buf.value()
            ))?;
            if nla.kind() == IFLA_VF_INFO {
                nlas.push(LinkVfInfo::parse(&NlaBuffer::new_checked(
                    nla.value(),
                )?)?);
            } else {
                log::warn!(
                    "BUG: Expecting IFLA_VF_INFO in IFLA_VFINFO_LIST, \
                    but got {}",
                    nla.kind()
                );
            }
        }
        Ok(Self(nlas))
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct LinkVfInfo(pub Vec<VfInfo>);

impl Nla for LinkVfInfo {
    fn value_len(&self) -> usize {
        self.0.as_slice().buffer_len()
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        self.0.as_slice().emit(buffer)
    }

    fn kind(&self) -> u16 {
        IFLA_VF_INFO
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for LinkVfInfo {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut nlas = vec![];
        for nla in NlasIterator::new(buf.into_inner()) {
            let nla = &nla.context(format!(
                "invalid IFLA_VF_INFO value {:?}",
                buf.value()
            ))?;
            nlas.push(VfInfo::parse(nla)?);
        }
        Ok(Self(nlas))
    }
}

const IFLA_VF_MAC: u16 = 1;
const IFLA_VF_VLAN: u16 = 2;
const IFLA_VF_TX_RATE: u16 = 3;
const IFLA_VF_SPOOFCHK: u16 = 4;
const IFLA_VF_LINK_STATE: u16 = 5;
const IFLA_VF_RATE: u16 = 6;
const IFLA_VF_RSS_QUERY_EN: u16 = 7;
const IFLA_VF_STATS: u16 = 8;
const IFLA_VF_TRUST: u16 = 9;
const IFLA_VF_IB_NODE_GUID: u16 = 10;
const IFLA_VF_IB_PORT_GUID: u16 = 11;
const IFLA_VF_VLAN_LIST: u16 = 12;
const IFLA_VF_BROADCAST: u16 = 13;

#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum VfInfo {
    Mac(VfInfoMac),
    Broadcast(VfInfoBroadcast),
    Vlan(VfInfoVlan),
    Rate(VfInfoRate),
    TxRate(VfInfoTxRate),
    SpoofCheck(VfInfoSpoofCheck),
    LinkState(VfInfoLinkState),
    RssQueryEn(VfInfoRssQueryEn),
    Trust(VfInfoTrust),
    IbNodeGuid(VfInfoGuid),
    IbPortGuid(VfInfoGuid),
    VlanList(Vec<VfVlan>),
    Stats(Vec<VfStats>),
    Other(DefaultNla),
}

impl Nla for VfInfo {
    fn value_len(&self) -> usize {
        match self {
            Self::Mac(v) => v.buffer_len(),
            Self::Vlan(v) => v.buffer_len(),
            Self::Broadcast(v) => v.buffer_len(),
            Self::Rate(v) => v.buffer_len(),
            Self::TxRate(v) => v.buffer_len(),
            Self::SpoofCheck(v) => v.buffer_len(),
            Self::LinkState(v) => v.buffer_len(),
            Self::RssQueryEn(v) => v.buffer_len(),
            Self::Trust(v) => v.buffer_len(),
            Self::IbNodeGuid(v) => v.buffer_len(),
            Self::IbPortGuid(v) => v.buffer_len(),
            Self::VlanList(v) => v.as_slice().buffer_len(),
            Self::Stats(v) => v.as_slice().buffer_len(),
            Self::Other(v) => v.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Mac(v) => v.emit(buffer),
            Self::Vlan(v) => v.emit(buffer),
            Self::Broadcast(v) => v.emit(buffer),
            Self::Rate(v) => v.emit(buffer),
            Self::TxRate(v) => v.emit(buffer),
            Self::SpoofCheck(v) => v.emit(buffer),
            Self::LinkState(v) => v.emit(buffer),
            Self::RssQueryEn(v) => v.emit(buffer),
            Self::Trust(v) => v.emit(buffer),
            Self::IbNodeGuid(v) => v.emit(buffer),
            Self::IbPortGuid(v) => v.emit(buffer),
            Self::VlanList(v) => v.as_slice().emit(buffer),
            Self::Stats(v) => v.as_slice().emit(buffer),
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Mac(_) => IFLA_VF_MAC,
            Self::Vlan(_) => IFLA_VF_VLAN,
            Self::Broadcast(_) => IFLA_VF_BROADCAST,
            Self::Rate(_) => IFLA_VF_RATE,
            Self::TxRate(_) => IFLA_VF_TX_RATE,
            Self::SpoofCheck(_) => IFLA_VF_SPOOFCHK,
            Self::LinkState(_) => IFLA_VF_LINK_STATE,
            Self::RssQueryEn(_) => IFLA_VF_RSS_QUERY_EN,
            Self::Trust(_) => IFLA_VF_TRUST,
            Self::IbNodeGuid(_) => IFLA_VF_IB_NODE_GUID,
            Self::IbPortGuid(_) => IFLA_VF_IB_PORT_GUID,
            Self::VlanList(_) => IFLA_VF_VLAN_LIST,
            Self::Stats(_) => IFLA_VF_STATS,
            Self::Other(v) => v.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for VfInfo {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_VF_MAC => Self::Mac(
                VfInfoMac::parse(&VfInfoMacBuffer::new(payload))
                    .context(format!("invalid IFLA_VF_MAC {payload:?}"))?,
            ),
            IFLA_VF_VLAN => Self::Vlan(
                VfInfoVlan::parse(&VfInfoVlanBuffer::new(payload))
                    .context(format!("invalid IFLA_VF_VLAN {payload:?}"))?,
            ),
            IFLA_VF_BROADCAST => Self::Broadcast(
                VfInfoBroadcast::parse(&VfInfoBroadcastBuffer::new(payload))
                    .context(format!(
                        "invalid IFLA_VF_BROADCAST {payload:?}"
                    ))?,
            ),
            IFLA_VF_RATE => Self::Rate(
                VfInfoRate::parse(&VfInfoRateBuffer::new(payload))
                    .context(format!("invalid IFLA_VF_RATE {payload:?}"))?,
            ),
            IFLA_VF_TX_RATE => Self::TxRate(
                VfInfoTxRate::parse(&VfInfoTxRateBuffer::new(payload))
                    .context(format!("invalid IFLA_VF_TX_RATE {payload:?}"))?,
            ),
            IFLA_VF_SPOOFCHK => Self::SpoofCheck(
                VfInfoSpoofCheck::parse(&VfInfoSpoofCheckBuffer::new(payload))
                    .context(format!("invalid IFLA_VF_SPOOFCHK {payload:?}"))?,
            ),
            IFLA_VF_LINK_STATE => Self::LinkState(
                VfInfoLinkState::parse(&VfInfoLinkStateBuffer::new(payload))
                    .context(format!(
                        "invalid IFLA_VF_LINK_STATE {payload:?}"
                    ))?,
            ),
            IFLA_VF_RSS_QUERY_EN => Self::RssQueryEn(
                VfInfoRssQueryEn::parse(&VfInfoRssQueryEnBuffer::new(payload))
                    .context(format!(
                        "invalid IFLA_VF_RSS_QUERY_EN {payload:?}"
                    ))?,
            ),
            IFLA_VF_TRUST => Self::Trust(
                VfInfoTrust::parse(&VfInfoTrustBuffer::new(payload))
                    .context(format!("invalid IFLA_VF_TRUST {payload:?}"))?,
            ),
            IFLA_VF_IB_NODE_GUID => Self::IbNodeGuid(
                VfInfoGuid::parse(&VfInfoGuidBuffer::new(payload)).context(
                    format!("invalid IFLA_VF_IB_NODE_GUID {payload:?}"),
                )?,
            ),
            IFLA_VF_IB_PORT_GUID => Self::IbPortGuid(
                VfInfoGuid::parse(&VfInfoGuidBuffer::new(payload)).context(
                    format!("invalid IFLA_VF_IB_PORT_GUID {payload:?}"),
                )?,
            ),
            IFLA_VF_VLAN_LIST => {
                let mut nlas: Vec<VfVlan> = Vec::new();
                for nla in NlasIterator::new(payload) {
                    let nla = &nla.context(format!(
                        "invalid IFLA_VF_VLAN_LIST value: {:?}",
                        buf.value()
                    ))?;

                    nlas.push(VfVlan::parse(nla)?);
                }
                Self::VlanList(nlas)
            }
            IFLA_VF_STATS => {
                let mut nlas: Vec<VfStats> = Vec::new();
                for nla in NlasIterator::new(payload) {
                    let nla = &nla.context(format!(
                        "invalid IFLA_VF_STATS value: {:?}",
                        buf.value()
                    ))?;

                    nlas.push(VfStats::parse(nla)?);
                }
                Self::Stats(nlas)
            }
            kind => Self::Other(DefaultNla::parse(buf).context(format!(
                "failed to parse {kind} as DefaultNla: {payload:?}"
            ))?),
        })
    }
}
