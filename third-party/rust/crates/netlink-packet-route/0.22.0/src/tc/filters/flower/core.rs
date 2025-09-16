// SPDX-License-Identifier: MIT

use super::TcFilterFlowerMplsOption;
use crate::ip::{parse_ipv4_addr, parse_ipv6_addr};
use crate::tc::TcAction;
use anyhow::Context;
use byteorder::{BigEndian, ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator, NLA_F_NESTED},
    parsers::{
        parse_mac, parse_u16, parse_u16_be, parse_u32, parse_u32_be, parse_u8,
    },
    traits::Emitable,
    DecodeError, Parseable,
};
use std::net::{Ipv4Addr, Ipv6Addr};

const TCA_FLOWER_CLASSID: u16 = 1;
const TCA_FLOWER_INDEV: u16 = 2;
const TCA_FLOWER_ACT: u16 = 3;
const TCA_FLOWER_KEY_ETH_DST: u16 = 4;
const TCA_FLOWER_KEY_ETH_DST_MASK: u16 = 5;
const TCA_FLOWER_KEY_ETH_SRC: u16 = 6;
const TCA_FLOWER_KEY_ETH_SRC_MASK: u16 = 7;
const TCA_FLOWER_KEY_ETH_TYPE: u16 = 8;
const TCA_FLOWER_KEY_IP_PROTO: u16 = 9;
const TCA_FLOWER_KEY_IPV4_SRC: u16 = 10;
const TCA_FLOWER_KEY_IPV4_SRC_MASK: u16 = 11;
const TCA_FLOWER_KEY_IPV4_DST: u16 = 12;
const TCA_FLOWER_KEY_IPV4_DST_MASK: u16 = 13;
const TCA_FLOWER_KEY_IPV6_SRC: u16 = 14;
const TCA_FLOWER_KEY_IPV6_SRC_MASK: u16 = 15;
const TCA_FLOWER_KEY_IPV6_DST: u16 = 16;
const TCA_FLOWER_KEY_IPV6_DST_MASK: u16 = 17;
const TCA_FLOWER_KEY_TCP_SRC: u16 = 18;
const TCA_FLOWER_KEY_TCP_DST: u16 = 19;
const TCA_FLOWER_KEY_UDP_SRC: u16 = 20;
const TCA_FLOWER_KEY_UDP_DST: u16 = 21;
const TCA_FLOWER_FLAGS: u16 = 22;
const TCA_FLOWER_KEY_VLAN_ID: u16 = 23;
const TCA_FLOWER_KEY_VLAN_PRIO: u16 = 24;
const TCA_FLOWER_KEY_VLAN_ETH_TYPE: u16 = 25;
const TCA_FLOWER_KEY_ENC_KEY_ID: u16 = 26;
const TCA_FLOWER_KEY_ENC_IPV4_SRC: u16 = 27;
const TCA_FLOWER_KEY_ENC_IPV4_SRC_MASK: u16 = 28;
const TCA_FLOWER_KEY_ENC_IPV4_DST: u16 = 29;
const TCA_FLOWER_KEY_ENC_IPV4_DST_MASK: u16 = 30;
const TCA_FLOWER_KEY_ENC_IPV6_SRC: u16 = 31;
const TCA_FLOWER_KEY_ENC_IPV6_SRC_MASK: u16 = 32;
const TCA_FLOWER_KEY_ENC_IPV6_DST: u16 = 33;
const TCA_FLOWER_KEY_ENC_IPV6_DST_MASK: u16 = 34;
const TCA_FLOWER_KEY_TCP_SRC_MASK: u16 = 35;
const TCA_FLOWER_KEY_TCP_DST_MASK: u16 = 36;
const TCA_FLOWER_KEY_UDP_SRC_MASK: u16 = 37;
const TCA_FLOWER_KEY_UDP_DST_MASK: u16 = 38;
const TCA_FLOWER_KEY_SCTP_SRC_MASK: u16 = 39;
const TCA_FLOWER_KEY_SCTP_DST_MASK: u16 = 40;
const TCA_FLOWER_KEY_SCTP_SRC: u16 = 41;
const TCA_FLOWER_KEY_SCTP_DST: u16 = 42;
const TCA_FLOWER_KEY_ENC_UDP_SRC_PORT: u16 = 43;
const TCA_FLOWER_KEY_ENC_UDP_SRC_PORT_MASK: u16 = 44;
const TCA_FLOWER_KEY_ENC_UDP_DST_PORT: u16 = 45;
const TCA_FLOWER_KEY_ENC_UDP_DST_PORT_MASK: u16 = 46;
const TCA_FLOWER_KEY_FLAGS: u16 = 47;
const TCA_FLOWER_KEY_FLAGS_MASK: u16 = 48;
const TCA_FLOWER_KEY_ICMPV4_CODE: u16 = 49;
const TCA_FLOWER_KEY_ICMPV4_CODE_MASK: u16 = 50;
const TCA_FLOWER_KEY_ICMPV4_TYPE: u16 = 51;
const TCA_FLOWER_KEY_ICMPV4_TYPE_MASK: u16 = 52;
const TCA_FLOWER_KEY_ICMPV6_CODE: u16 = 53;
const TCA_FLOWER_KEY_ICMPV6_CODE_MASK: u16 = 54;
const TCA_FLOWER_KEY_ICMPV6_TYPE: u16 = 55;
const TCA_FLOWER_KEY_ICMPV6_TYPE_MASK: u16 = 56;
const TCA_FLOWER_KEY_ARP_SIP: u16 = 57;
const TCA_FLOWER_KEY_ARP_SIP_MASK: u16 = 58;
const TCA_FLOWER_KEY_ARP_TIP: u16 = 59;
const TCA_FLOWER_KEY_ARP_TIP_MASK: u16 = 60;
const TCA_FLOWER_KEY_ARP_OP: u16 = 61;
const TCA_FLOWER_KEY_ARP_OP_MASK: u16 = 62;
const TCA_FLOWER_KEY_ARP_SHA: u16 = 63;
const TCA_FLOWER_KEY_ARP_SHA_MASK: u16 = 64;
const TCA_FLOWER_KEY_ARP_THA: u16 = 65;
const TCA_FLOWER_KEY_ARP_THA_MASK: u16 = 66;
const TCA_FLOWER_KEY_MPLS_TTL: u16 = 67;
const TCA_FLOWER_KEY_MPLS_BOS: u16 = 68;
const TCA_FLOWER_KEY_MPLS_TC: u16 = 69;
const TCA_FLOWER_KEY_MPLS_LABEL: u16 = 70;
const TCA_FLOWER_KEY_TCP_FLAGS: u16 = 71;
const TCA_FLOWER_KEY_TCP_FLAGS_MASK: u16 = 72;
const TCA_FLOWER_KEY_IP_TOS: u16 = 73;
const TCA_FLOWER_KEY_IP_TOS_MASK: u16 = 74;
const TCA_FLOWER_KEY_IP_TTL: u16 = 75;
const TCA_FLOWER_KEY_IP_TTL_MASK: u16 = 76;
const TCA_FLOWER_KEY_CVLAN_ID: u16 = 77;
const TCA_FLOWER_KEY_CVLAN_PRIO: u16 = 78;
const TCA_FLOWER_KEY_CVLAN_ETH_TYPE: u16 = 79;
const TCA_FLOWER_KEY_ENC_IP_TOS: u16 = 80;
const TCA_FLOWER_KEY_ENC_IP_TOS_MASK: u16 = 81;
const TCA_FLOWER_KEY_ENC_IP_TTL: u16 = 82;
const TCA_FLOWER_KEY_ENC_IP_TTL_MASK: u16 = 83;
// const TCA_FLOWER_KEY_ENC_OPTS: u16 = 84;
// const TCA_FLOWER_KEY_ENC_OPTS_MASK: u16 = 85;
const TCA_FLOWER_IN_HW_COUNT: u16 = 86;
const TCA_FLOWER_KEY_PORT_SRC_MIN: u16 = 87;
const TCA_FLOWER_KEY_PORT_SRC_MAX: u16 = 88;
const TCA_FLOWER_KEY_PORT_DST_MIN: u16 = 89;
const TCA_FLOWER_KEY_PORT_DST_MAX: u16 = 90;
const TCA_FLOWER_KEY_CT_STATE: u16 = 91;
const TCA_FLOWER_KEY_CT_STATE_MASK: u16 = 92;
const TCA_FLOWER_KEY_CT_ZONE: u16 = 93;
const TCA_FLOWER_KEY_CT_ZONE_MASK: u16 = 94;
const TCA_FLOWER_KEY_CT_MARK: u16 = 95;
const TCA_FLOWER_KEY_CT_MARK_MASK: u16 = 96;
const TCA_FLOWER_KEY_CT_LABELS: u16 = 97;
const TCA_FLOWER_KEY_CT_LABELS_MASK: u16 = 98;
const TCA_FLOWER_KEY_MPLS_OPTS: u16 = 99;
const TCA_FLOWER_KEY_HASH: u16 = 100;
const TCA_FLOWER_KEY_HASH_MASK: u16 = 101;

fn parse_bytes_16(payload: &[u8]) -> Result<[u8; 16], DecodeError> {
    if payload.len() != 16 {
        return Err(format!("invalid payload size: {payload:?}").into());
    }
    let mut data = [0x00; 16];
    for (i, byte) in payload.iter().enumerate() {
        data[i] = *byte;
    }
    Ok(data)
}

macro_rules! nla_err {
    // Match rule that takes an argument expression
    ($message:expr) => {
        format!("failed to parse {} value", stringify!($message))
    };
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub struct TcFilterFlower {}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum TcFilterFlowerOption {
    ClassId(u32),
    InDev(u32),
    Actions(Vec<TcAction>),
    EthDst([u8; 6]),
    EthDstMask([u8; 6]),
    EthSrc([u8; 6]),
    EthSrcMask([u8; 6]),
    EthType(u16),
    IpProto(u8),
    IpTtl(u8),
    IpTtlMask(u8),
    IpTos(u8),
    IpTosMask(u8),
    Ipv4Src(Ipv4Addr),
    Ipv4SrcMask(Ipv4Addr),
    Ipv4Dst(Ipv4Addr),
    Ipv4DstMask(Ipv4Addr),
    Ipv6Src(Ipv6Addr),
    Ipv6SrcMask(Ipv6Addr),
    Ipv6Dst(Ipv6Addr),
    Ipv6DstMask(Ipv6Addr),
    TcpSrc(u16),
    TcpDst(u16),
    UdpSrc(u16),
    UdpDst(u16),
    SctpSrc(u16),
    SctpDst(u16),
    TcpSrcMask(u16),
    TcpDstMask(u16),
    UdpSrcMask(u16),
    UdpDstMask(u16),
    SctpSrcMask(u16),
    SctpDstMask(u16),
    Icmpv4Code(u8),
    Icmpv4CodeMask(u8),
    Icmpv4Type(u8),
    Icmpv4TypeMask(u8),
    Icmpv6Code(u8),
    Icmpv6CodeMask(u8),
    Icmpv6Type(u8),
    Icmpv6TypeMask(u8),
    ArpSip(Ipv4Addr),
    ArpSipMask(Ipv4Addr),
    ArpTip(Ipv4Addr),
    ArpTipMask(Ipv4Addr),
    ArpOp(u8),
    ArpOpMask(u8),
    ArpSha([u8; 6]),
    ArpShaMask([u8; 6]),
    ArpTha([u8; 6]),
    ArpThaMask([u8; 6]),
    MplsTtl(u8),
    MplsBos(u8),
    MplsTc(u8),
    MplsLabel(u32),
    TcpFlags(u16),
    TcpFlagsMask(u16),
    KeyFlags(u32),
    KeyFlagsMask(u32),

    Flags(u32),
    VlanId(u16),
    VlanPrio(u8),
    VlanEthType(u16),

    CvlanId(u16),
    CvlanPrio(u8),
    CvlanEthType(u16),
    EncKeyId(u32),
    EncKeyUdpSrcPort(u16),
    EncKeyUdpSrcPortMask(u16),
    EncKeyUdpDstPort(u16),
    EncKeyUdpDstPortMask(u16),
    EncKeyIpTtl(u8),
    EncKeyIpTtlMask(u8),
    EncKeyIpTos(u8),
    EncKeyIpTosMask(u8),
    EncKeyIpv4Src(Ipv4Addr),
    EncKeyIpv4SrcMask(Ipv4Addr),
    EncKeyIpv4Dst(Ipv4Addr),
    EncKeyIpv4DstMask(Ipv4Addr),
    EncKeyIpv6Src(Ipv6Addr),
    EncKeyIpv6SrcMask(Ipv6Addr),
    EncKeyIpv6Dst(Ipv6Addr),
    EncKeyIpv6DstMask(Ipv6Addr),
    InHwCount(u32),
    PortSrcMin(u16),
    PortSrcMax(u16),
    PortDstMin(u16),
    PortDstMax(u16),
    CtState(u16),
    CtStateMask(u16),
    CtZone(u16),
    CtZoneMask(u16),
    CtMark(u32),
    CtMarkMask(u32),
    CtLabels([u8; 16]),
    CtLabelsMask([u8; 16]),
    MplsOpts(Vec<TcFilterFlowerMplsOption>),
    KeyHash(u32),
    KeyHashMask(u32),

    Other(DefaultNla),
}

impl TcFilterFlower {
    pub const KIND: &'static str = "flower";
}

impl Nla for TcFilterFlowerOption {
    fn value_len(&self) -> usize {
        match self {
            Self::ClassId(_) => 4,
            Self::InDev(_) => 4,
            Self::Actions(acts) => acts.as_slice().buffer_len(),
            Self::EthDst(_)
            | Self::EthDstMask(_)
            | Self::EthSrc(_)
            | Self::EthSrcMask(_) => 6,
            Self::EthType(_) => 2,
            Self::IpProto(_) => 1,
            Self::IpTtl(_)
            | Self::IpTtlMask(_)
            | Self::IpTos(_)
            | Self::IpTosMask(_) => 1,
            Self::Ipv4Src(_)
            | Self::Ipv4SrcMask(_)
            | Self::Ipv4Dst(_)
            | Self::Ipv4DstMask(_) => 4,
            Self::Ipv6Src(_)
            | Self::Ipv6SrcMask(_)
            | Self::Ipv6Dst(_)
            | Self::Ipv6DstMask(_) => 16,
            Self::TcpDst(_)
            | Self::TcpSrc(_)
            | Self::UdpDst(_)
            | Self::UdpSrc(_)
            | Self::SctpDst(_)
            | Self::SctpSrc(_)
            | Self::TcpDstMask(_)
            | Self::TcpSrcMask(_)
            | Self::UdpDstMask(_)
            | Self::UdpSrcMask(_)
            | Self::SctpDstMask(_)
            | Self::SctpSrcMask(_) => 2,
            Self::Icmpv4Code(_)
            | Self::Icmpv4CodeMask(_)
            | Self::Icmpv4Type(_)
            | Self::Icmpv4TypeMask(_)
            | Self::Icmpv6Code(_)
            | Self::Icmpv6CodeMask(_)
            | Self::Icmpv6Type(_)
            | Self::Icmpv6TypeMask(_) => 1,
            Self::ArpSip(_)
            | Self::ArpSipMask(_)
            | Self::ArpTip(_)
            | Self::ArpTipMask(_) => 4,
            Self::ArpOp(_) | Self::ArpOpMask(_) => 1,
            Self::ArpSha(_)
            | Self::ArpShaMask(_)
            | Self::ArpTha(_)
            | Self::ArpThaMask(_) => 6,
            Self::MplsTtl(_) | Self::MplsBos(_) | Self::MplsTc(_) => 1,
            Self::MplsLabel(_) => 4,
            Self::TcpFlags(_) | Self::TcpFlagsMask(_) => 2,
            Self::KeyFlags(_) | Self::KeyFlagsMask(_) => 4,

            Self::Flags(_) => 4,
            Self::VlanId(_) => 2,
            Self::VlanPrio(_) => 1,
            Self::VlanEthType(_) => 2,
            Self::CvlanId(_) => 2,
            Self::CvlanPrio(_) => 1,
            Self::CvlanEthType(_) => 2,
            Self::EncKeyId(_) => 4,
            Self::EncKeyUdpSrcPort(_)
            | Self::EncKeyUdpSrcPortMask(_)
            | Self::EncKeyUdpDstPort(_)
            | Self::EncKeyUdpDstPortMask(_) => 2,
            Self::EncKeyIpTtl(_)
            | Self::EncKeyIpTtlMask(_)
            | Self::EncKeyIpTos(_)
            | Self::EncKeyIpTosMask(_) => 1,
            Self::EncKeyIpv4Src(_)
            | Self::EncKeyIpv4SrcMask(_)
            | Self::EncKeyIpv4Dst(_)
            | Self::EncKeyIpv4DstMask(_) => 4,
            Self::EncKeyIpv6Src(_)
            | Self::EncKeyIpv6SrcMask(_)
            | Self::EncKeyIpv6Dst(_)
            | Self::EncKeyIpv6DstMask(_) => 16,
            Self::InHwCount(_) => 4,
            Self::PortSrcMin(_)
            | Self::PortSrcMax(_)
            | Self::PortDstMin(_)
            | Self::PortDstMax(_) => 2,
            Self::CtState(_)
            | Self::CtStateMask(_)
            | Self::CtZone(_)
            | Self::CtZoneMask(_) => 2,
            Self::CtMark(_) | Self::CtMarkMask(_) => 4,
            Self::CtLabels(_) | Self::CtLabelsMask(_) => 16,
            Self::MplsOpts(attr) => attr.as_slice().buffer_len(),
            Self::KeyHash(_) | Self::KeyHashMask(_) => 4,

            Self::Other(attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::ClassId(i) => NativeEndian::write_u32(buffer, *i),
            Self::InDev(i) => NativeEndian::write_u32(buffer, *i),
            Self::Actions(acts) => acts.as_slice().emit(buffer),
            Self::EthDst(b)
            | Self::EthDstMask(b)
            | Self::EthSrc(b)
            | Self::EthSrcMask(b) => buffer.copy_from_slice(b.as_slice()),
            Self::EthType(i) => BigEndian::write_u16(buffer, *i),
            Self::IpProto(i) => buffer[0] = *i,
            Self::IpTtl(i)
            | Self::IpTtlMask(i)
            | Self::IpTos(i)
            | Self::IpTosMask(i) => buffer[0] = *i,
            Self::Ipv4Src(ip)
            | Self::Ipv4SrcMask(ip)
            | Self::Ipv4Dst(ip)
            | Self::Ipv4DstMask(ip) => buffer.copy_from_slice(&ip.octets()),
            Self::Ipv6Src(ip)
            | Self::Ipv6SrcMask(ip)
            | Self::Ipv6Dst(ip)
            | Self::Ipv6DstMask(ip) => buffer.copy_from_slice(&ip.octets()),
            Self::TcpSrc(i)
            | Self::TcpDst(i)
            | Self::UdpSrc(i)
            | Self::UdpDst(i)
            | Self::SctpSrc(i)
            | Self::SctpDst(i)
            | Self::TcpSrcMask(i)
            | Self::TcpDstMask(i)
            | Self::UdpSrcMask(i)
            | Self::UdpDstMask(i)
            | Self::SctpSrcMask(i)
            | Self::SctpDstMask(i) => BigEndian::write_u16(buffer, *i),
            Self::Icmpv4Code(i)
            | Self::Icmpv4CodeMask(i)
            | Self::Icmpv4Type(i)
            | Self::Icmpv4TypeMask(i)
            | Self::Icmpv6Code(i)
            | Self::Icmpv6CodeMask(i)
            | Self::Icmpv6Type(i)
            | Self::Icmpv6TypeMask(i) => buffer[0] = *i,
            Self::ArpSip(ip)
            | Self::ArpSipMask(ip)
            | Self::ArpTip(ip)
            | Self::ArpTipMask(ip) => buffer.copy_from_slice(&ip.octets()),
            Self::ArpOp(i) | Self::ArpOpMask(i) => buffer[0] = *i,
            Self::ArpSha(b)
            | Self::ArpShaMask(b)
            | Self::ArpTha(b)
            | Self::ArpThaMask(b) => buffer.copy_from_slice(b.as_slice()),
            Self::MplsTtl(i) => buffer[0] = *i,
            Self::MplsBos(i) => buffer[0] = *i & 0x01,
            Self::MplsTc(i) => buffer[0] = *i & 0x07,
            Self::MplsLabel(i) => NativeEndian::write_u32(buffer, *i & 0xFFFFF),
            Self::TcpFlags(i) | Self::TcpFlagsMask(i) => {
                BigEndian::write_u16(buffer, *i)
            }
            Self::KeyFlags(i) | Self::KeyFlagsMask(i) => {
                BigEndian::write_u32(buffer, *i)
            }
            Self::Flags(i) => NativeEndian::write_u32(buffer, *i),
            Self::VlanId(i) => NativeEndian::write_u16(buffer, *i),
            Self::VlanPrio(i) => buffer[0] = *i,
            Self::VlanEthType(i) => BigEndian::write_u16(buffer, *i),
            Self::CvlanId(i) => NativeEndian::write_u16(buffer, *i),
            Self::CvlanPrio(i) => buffer[0] = *i,
            Self::CvlanEthType(i) => BigEndian::write_u16(buffer, *i),
            Self::EncKeyId(i) => BigEndian::write_u32(buffer, *i),
            Self::EncKeyIpTtl(i)
            | Self::EncKeyIpTtlMask(i)
            | Self::EncKeyIpTos(i)
            | Self::EncKeyIpTosMask(i) => buffer[0] = *i,
            Self::EncKeyUdpSrcPort(i)
            | Self::EncKeyUdpSrcPortMask(i)
            | Self::EncKeyUdpDstPort(i)
            | Self::EncKeyUdpDstPortMask(i) => BigEndian::write_u16(buffer, *i),
            Self::EncKeyIpv4Src(ip)
            | Self::EncKeyIpv4SrcMask(ip)
            | Self::EncKeyIpv4Dst(ip)
            | Self::EncKeyIpv4DstMask(ip) => {
                buffer.copy_from_slice(&ip.octets())
            }
            Self::EncKeyIpv6Src(ip)
            | Self::EncKeyIpv6SrcMask(ip)
            | Self::EncKeyIpv6Dst(ip)
            | Self::EncKeyIpv6DstMask(ip) => {
                buffer.copy_from_slice(&ip.octets())
            }
            Self::InHwCount(i) => NativeEndian::write_u32(buffer, *i),
            Self::PortSrcMin(i)
            | Self::PortSrcMax(i)
            | Self::PortDstMin(i)
            | Self::PortDstMax(i) => BigEndian::write_u16(buffer, *i),
            Self::CtState(i)
            | Self::CtStateMask(i)
            | Self::CtZone(i)
            | Self::CtZoneMask(i) => NativeEndian::write_u16(buffer, *i),
            Self::CtMark(i) | Self::CtMarkMask(i) => {
                NativeEndian::write_u32(buffer, *i)
            }
            Self::CtLabels(b) | Self::CtLabelsMask(b) => {
                buffer.copy_from_slice(b.as_slice())
            }
            Self::MplsOpts(attr) => attr.as_slice().emit(buffer),
            Self::KeyHash(i) | Self::KeyHashMask(i) => {
                NativeEndian::write_u32(buffer, *i)
            }
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::ClassId(_) => TCA_FLOWER_CLASSID,
            Self::InDev(_) => TCA_FLOWER_INDEV,
            Self::Actions(_) => TCA_FLOWER_ACT,
            Self::EthDst(_) => TCA_FLOWER_KEY_ETH_DST,
            Self::EthDstMask(_) => TCA_FLOWER_KEY_ETH_DST_MASK,
            Self::EthSrc(_) => TCA_FLOWER_KEY_ETH_SRC,
            Self::EthSrcMask(_) => TCA_FLOWER_KEY_ETH_SRC_MASK,
            Self::EthType(_) => TCA_FLOWER_KEY_ETH_TYPE,
            Self::IpProto(_) => TCA_FLOWER_KEY_IP_PROTO,
            Self::IpTtl(_) => TCA_FLOWER_KEY_IP_TTL,
            Self::IpTtlMask(_) => TCA_FLOWER_KEY_IP_TTL_MASK,
            Self::IpTos(_) => TCA_FLOWER_KEY_IP_TOS,
            Self::IpTosMask(_) => TCA_FLOWER_KEY_IP_TOS_MASK,
            Self::Ipv4Src(_) => TCA_FLOWER_KEY_IPV4_SRC,
            Self::Ipv4SrcMask(_) => TCA_FLOWER_KEY_IPV4_SRC_MASK,
            Self::Ipv4Dst(_) => TCA_FLOWER_KEY_IPV4_DST,
            Self::Ipv4DstMask(_) => TCA_FLOWER_KEY_IPV4_DST_MASK,
            Self::Ipv6Src(_) => TCA_FLOWER_KEY_IPV6_SRC,
            Self::Ipv6SrcMask(_) => TCA_FLOWER_KEY_IPV6_SRC_MASK,
            Self::Ipv6Dst(_) => TCA_FLOWER_KEY_IPV6_DST,
            Self::Ipv6DstMask(_) => TCA_FLOWER_KEY_IPV6_DST_MASK,
            Self::TcpSrc(_) => TCA_FLOWER_KEY_TCP_SRC,
            Self::TcpDst(_) => TCA_FLOWER_KEY_TCP_DST,
            Self::TcpSrcMask(_) => TCA_FLOWER_KEY_TCP_SRC_MASK,
            Self::TcpDstMask(_) => TCA_FLOWER_KEY_TCP_DST_MASK,
            Self::UdpSrc(_) => TCA_FLOWER_KEY_UDP_SRC,
            Self::UdpDst(_) => TCA_FLOWER_KEY_UDP_DST,
            Self::UdpSrcMask(_) => TCA_FLOWER_KEY_UDP_SRC_MASK,
            Self::UdpDstMask(_) => TCA_FLOWER_KEY_UDP_DST_MASK,
            Self::SctpSrc(_) => TCA_FLOWER_KEY_SCTP_SRC,
            Self::SctpDst(_) => TCA_FLOWER_KEY_SCTP_DST,
            Self::SctpSrcMask(_) => TCA_FLOWER_KEY_SCTP_SRC_MASK,
            Self::SctpDstMask(_) => TCA_FLOWER_KEY_SCTP_DST_MASK,
            Self::Flags(_) => TCA_FLOWER_FLAGS,
            Self::VlanId(_) => TCA_FLOWER_KEY_VLAN_ID,
            Self::VlanPrio(_) => TCA_FLOWER_KEY_VLAN_PRIO,
            Self::VlanEthType(_) => TCA_FLOWER_KEY_VLAN_ETH_TYPE,
            Self::EncKeyId(_) => TCA_FLOWER_KEY_ENC_KEY_ID,
            Self::EncKeyUdpSrcPort(_) => TCA_FLOWER_KEY_ENC_UDP_SRC_PORT,
            Self::EncKeyUdpSrcPortMask(_) => {
                TCA_FLOWER_KEY_ENC_UDP_SRC_PORT_MASK
            }
            Self::EncKeyUdpDstPort(_) => TCA_FLOWER_KEY_ENC_UDP_DST_PORT,
            Self::EncKeyUdpDstPortMask(_) => {
                TCA_FLOWER_KEY_ENC_UDP_DST_PORT_MASK
            }
            Self::Icmpv4Code(_) => TCA_FLOWER_KEY_ICMPV4_CODE,
            Self::Icmpv4CodeMask(_) => TCA_FLOWER_KEY_ICMPV4_CODE_MASK,
            Self::Icmpv4Type(_) => TCA_FLOWER_KEY_ICMPV4_TYPE,
            Self::Icmpv4TypeMask(_) => TCA_FLOWER_KEY_ICMPV4_TYPE_MASK,
            Self::Icmpv6Code(_) => TCA_FLOWER_KEY_ICMPV6_CODE,
            Self::Icmpv6CodeMask(_) => TCA_FLOWER_KEY_ICMPV6_CODE_MASK,
            Self::Icmpv6Type(_) => TCA_FLOWER_KEY_ICMPV6_TYPE,
            Self::Icmpv6TypeMask(_) => TCA_FLOWER_KEY_ICMPV6_TYPE_MASK,
            Self::ArpSip(_) => TCA_FLOWER_KEY_ARP_SIP,
            Self::ArpSipMask(_) => TCA_FLOWER_KEY_ARP_SIP_MASK,
            Self::ArpTip(_) => TCA_FLOWER_KEY_ARP_TIP,
            Self::ArpTipMask(_) => TCA_FLOWER_KEY_ARP_TIP_MASK,
            Self::ArpOp(_) => TCA_FLOWER_KEY_ARP_OP,
            Self::ArpOpMask(_) => TCA_FLOWER_KEY_ARP_OP_MASK,
            Self::ArpSha(_) => TCA_FLOWER_KEY_ARP_SHA,
            Self::ArpShaMask(_) => TCA_FLOWER_KEY_ARP_SHA_MASK,
            Self::ArpTha(_) => TCA_FLOWER_KEY_ARP_THA,
            Self::ArpThaMask(_) => TCA_FLOWER_KEY_ARP_THA_MASK,
            Self::MplsTtl(_) => TCA_FLOWER_KEY_MPLS_TTL,
            Self::MplsBos(_) => TCA_FLOWER_KEY_MPLS_BOS,
            Self::MplsTc(_) => TCA_FLOWER_KEY_MPLS_TC,
            Self::MplsLabel(_) => TCA_FLOWER_KEY_MPLS_LABEL,
            Self::TcpFlags(_) => TCA_FLOWER_KEY_TCP_FLAGS,
            Self::TcpFlagsMask(_) => TCA_FLOWER_KEY_TCP_FLAGS_MASK,
            Self::KeyFlags(_) => TCA_FLOWER_KEY_FLAGS,
            Self::KeyFlagsMask(_) => TCA_FLOWER_KEY_FLAGS_MASK,
            Self::CvlanId(_) => TCA_FLOWER_KEY_CVLAN_ID,
            Self::CvlanPrio(_) => TCA_FLOWER_KEY_CVLAN_PRIO,
            Self::CvlanEthType(_) => TCA_FLOWER_KEY_CVLAN_ETH_TYPE,
            Self::EncKeyIpTtl(_) => TCA_FLOWER_KEY_ENC_IP_TTL,
            Self::EncKeyIpTtlMask(_) => TCA_FLOWER_KEY_ENC_IP_TTL_MASK,
            Self::EncKeyIpTos(_) => TCA_FLOWER_KEY_ENC_IP_TOS,
            Self::EncKeyIpTosMask(_) => TCA_FLOWER_KEY_ENC_IP_TOS_MASK,
            Self::EncKeyIpv4Src(_) => TCA_FLOWER_KEY_ENC_IPV4_SRC,
            Self::EncKeyIpv4SrcMask(_) => TCA_FLOWER_KEY_ENC_IPV4_SRC_MASK,
            Self::EncKeyIpv4Dst(_) => TCA_FLOWER_KEY_ENC_IPV4_DST,
            Self::EncKeyIpv4DstMask(_) => TCA_FLOWER_KEY_ENC_IPV4_DST_MASK,
            Self::EncKeyIpv6Src(_) => TCA_FLOWER_KEY_ENC_IPV6_SRC,
            Self::EncKeyIpv6SrcMask(_) => TCA_FLOWER_KEY_ENC_IPV6_SRC_MASK,
            Self::EncKeyIpv6Dst(_) => TCA_FLOWER_KEY_ENC_IPV6_DST,
            Self::EncKeyIpv6DstMask(_) => TCA_FLOWER_KEY_ENC_IPV6_DST_MASK,
            Self::InHwCount(_) => TCA_FLOWER_IN_HW_COUNT,
            Self::PortSrcMin(_) => TCA_FLOWER_KEY_PORT_SRC_MIN,
            Self::PortSrcMax(_) => TCA_FLOWER_KEY_PORT_SRC_MAX,
            Self::PortDstMin(_) => TCA_FLOWER_KEY_PORT_DST_MIN,
            Self::PortDstMax(_) => TCA_FLOWER_KEY_PORT_DST_MAX,
            Self::CtState(_) => TCA_FLOWER_KEY_CT_STATE,
            Self::CtStateMask(_) => TCA_FLOWER_KEY_CT_STATE_MASK,
            Self::CtZone(_) => TCA_FLOWER_KEY_CT_ZONE,
            Self::CtZoneMask(_) => TCA_FLOWER_KEY_CT_ZONE_MASK,
            Self::CtMark(_) => TCA_FLOWER_KEY_CT_MARK,
            Self::CtMarkMask(_) => TCA_FLOWER_KEY_CT_MARK_MASK,
            Self::CtLabels(_) => TCA_FLOWER_KEY_CT_LABELS,
            Self::CtLabelsMask(_) => TCA_FLOWER_KEY_CT_LABELS_MASK,
            Self::MplsOpts(_) => TCA_FLOWER_KEY_MPLS_OPTS | NLA_F_NESTED,
            Self::KeyHash(_) => TCA_FLOWER_KEY_HASH,
            Self::KeyHashMask(_) => TCA_FLOWER_KEY_HASH_MASK,
            Self::Other(attr) => attr.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for TcFilterFlowerOption
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            // TCA_FLOWER_CLASSID => Self::ClassId(TcHandle::from(
            //    parse_u32(payload).context("failed to parse
            // TCA_FLOWER_CLASSID")?, )),
            TCA_FLOWER_INDEV => Self::InDev(
                parse_u32(payload).context(nla_err!(TCA_FLOWER_INDEV))?,
            ),
            TCA_FLOWER_ACT => {
                let mut acts = vec![];
                for act in NlasIterator::new(payload) {
                    let act = act.context("invalid TCA_FLOWER_ACT")?;
                    acts.push(
                        TcAction::parse(&act)
                            .context(nla_err!(TCA_FLOWER_ACT))?,
                    );
                }
                Self::Actions(acts)
            }
            TCA_FLOWER_KEY_ETH_DST => Self::EthDst(
                parse_mac(payload).context(nla_err!(TCA_FLOWER_KEY_ETH_DST))?,
            ),
            TCA_FLOWER_KEY_ETH_DST_MASK => Self::EthDstMask(
                parse_mac(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ETH_DST_MASK))?,
            ),
            TCA_FLOWER_KEY_ETH_SRC => Self::EthSrc(
                parse_mac(payload).context(nla_err!(TCA_FLOWER_KEY_ETH_SRC))?,
            ),
            TCA_FLOWER_KEY_ETH_SRC_MASK => Self::EthSrcMask(
                parse_mac(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ETH_SRC_MASK))?,
            ),
            TCA_FLOWER_KEY_ETH_TYPE => Self::EthType(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ETH_TYPE))?,
            ),
            TCA_FLOWER_KEY_IP_PROTO => Self::IpProto(
                parse_u8(payload).context(nla_err!(TCA_FLOWER_KEY_IP_PROTO))?,
            ),
            TCA_FLOWER_KEY_IP_TTL => Self::IpTtl(
                parse_u8(payload).context(nla_err!(TCA_FLOWER_KEY_IP_TTL))?,
            ),
            TCA_FLOWER_KEY_IP_TTL_MASK => Self::IpTtlMask(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_IP_TTL_MASK))?,
            ),
            TCA_FLOWER_KEY_IP_TOS => Self::IpTos(
                parse_u8(payload).context(nla_err!(TCA_FLOWER_KEY_IP_TOS))?,
            ),
            TCA_FLOWER_KEY_IP_TOS_MASK => Self::IpTosMask(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_IP_TOS_MASK))?,
            ),
            TCA_FLOWER_KEY_IPV4_SRC => Self::Ipv4Src(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_IPV4_SRC))?,
            ),
            TCA_FLOWER_KEY_IPV4_SRC_MASK => Self::Ipv4SrcMask(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_IPV4_SRC_MASK))?,
            ),
            TCA_FLOWER_KEY_IPV4_DST => Self::Ipv4Dst(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_IPV4_DST))?,
            ),
            TCA_FLOWER_KEY_IPV4_DST_MASK => Self::Ipv4DstMask(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_IPV4_DST_MASK))?,
            ),
            TCA_FLOWER_KEY_IPV6_SRC => Self::Ipv6Src(
                parse_ipv6_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_IPV6_SRC))?,
            ),
            TCA_FLOWER_KEY_IPV6_SRC_MASK => Self::Ipv6SrcMask(
                parse_ipv6_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_IPV6_SRC_MASK))?,
            ),
            TCA_FLOWER_KEY_IPV6_DST => Self::Ipv6Dst(
                parse_ipv6_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_IPV6_DST))?,
            ),
            TCA_FLOWER_KEY_IPV6_DST_MASK => Self::Ipv6DstMask(
                parse_ipv6_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_IPV6_DST_MASK))?,
            ),
            TCA_FLOWER_KEY_TCP_SRC => Self::TcpSrc(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_TCP_SRC))?,
            ),
            TCA_FLOWER_KEY_TCP_DST => Self::TcpDst(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_TCP_DST))?,
            ),
            TCA_FLOWER_KEY_TCP_SRC_MASK => Self::TcpSrcMask(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_TCP_SRC_MASK))?,
            ),
            TCA_FLOWER_KEY_TCP_DST_MASK => Self::TcpDstMask(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_TCP_DST_MASK))?,
            ),
            TCA_FLOWER_KEY_UDP_SRC => Self::UdpSrc(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_UDP_SRC))?,
            ),
            TCA_FLOWER_KEY_UDP_DST => Self::UdpDst(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_UDP_DST))?,
            ),
            TCA_FLOWER_KEY_UDP_SRC_MASK => Self::UdpSrcMask(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_UDP_SRC_MASK))?,
            ),
            TCA_FLOWER_KEY_UDP_DST_MASK => Self::UdpDstMask(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_UDP_DST_MASK))?,
            ),
            TCA_FLOWER_KEY_SCTP_SRC => Self::SctpSrc(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_SCTP_SRC))?,
            ),
            TCA_FLOWER_KEY_SCTP_DST => Self::SctpDst(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_SCTP_DST))?,
            ),
            TCA_FLOWER_KEY_SCTP_SRC_MASK => Self::SctpSrcMask(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_SCTP_SRC_MASK))?,
            ),
            TCA_FLOWER_KEY_SCTP_DST_MASK => Self::SctpDstMask(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_SCTP_DST_MASK))?,
            ),

            TCA_FLOWER_FLAGS => Self::Flags(
                parse_u32(payload).context(nla_err!(TCA_FLOWER_FLAGS))?,
            ),

            TCA_FLOWER_KEY_VLAN_ID => Self::VlanId(
                parse_u16(payload).context(nla_err!(TCA_FLOWER_KEY_VLAN_ID))?,
            ),
            TCA_FLOWER_KEY_VLAN_PRIO => Self::VlanPrio(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_VLAN_PRIO))?,
            ),
            TCA_FLOWER_KEY_VLAN_ETH_TYPE => Self::VlanEthType(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_VLAN_ETH_TYPE))?,
            ),
            TCA_FLOWER_KEY_CVLAN_ID => Self::CvlanId(
                parse_u16(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_CVLAN_ID))?,
            ),
            TCA_FLOWER_KEY_CVLAN_PRIO => Self::CvlanPrio(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_CVLAN_PRIO))?,
            ),
            TCA_FLOWER_KEY_CVLAN_ETH_TYPE => Self::CvlanEthType(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_CVLAN_ETH_TYPE))?,
            ),
            TCA_FLOWER_KEY_ENC_KEY_ID => Self::EncKeyId(
                parse_u32_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_KEY_ID))?,
            ),
            TCA_FLOWER_KEY_ENC_UDP_SRC_PORT => Self::EncKeyUdpSrcPort(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_UDP_SRC_PORT))?,
            ),
            TCA_FLOWER_KEY_ENC_UDP_SRC_PORT_MASK => Self::EncKeyUdpSrcPortMask(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_UDP_SRC_PORT_MASK))?,
            ),
            TCA_FLOWER_KEY_ENC_UDP_DST_PORT => Self::EncKeyUdpDstPort(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_UDP_DST_PORT))?,
            ),
            TCA_FLOWER_KEY_ENC_UDP_DST_PORT_MASK => Self::EncKeyUdpDstPortMask(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_UDP_DST_PORT_MASK))?,
            ),
            TCA_FLOWER_KEY_ICMPV4_CODE => Self::Icmpv4Code(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ICMPV4_CODE))?,
            ),
            TCA_FLOWER_KEY_ICMPV4_CODE_MASK => Self::Icmpv4CodeMask(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ICMPV4_CODE_MASK))?,
            ),
            TCA_FLOWER_KEY_ICMPV4_TYPE => Self::Icmpv4Type(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ICMPV4_TYPE))?,
            ),
            TCA_FLOWER_KEY_ICMPV4_TYPE_MASK => Self::Icmpv4TypeMask(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ICMPV4_TYPE_MASK))?,
            ),
            TCA_FLOWER_KEY_ICMPV6_CODE => Self::Icmpv6Code(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ICMPV6_CODE))?,
            ),
            TCA_FLOWER_KEY_ICMPV6_CODE_MASK => Self::Icmpv6CodeMask(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ICMPV6_CODE_MASK))?,
            ),
            TCA_FLOWER_KEY_ICMPV6_TYPE => Self::Icmpv6Type(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ICMPV6_TYPE))?,
            ),
            TCA_FLOWER_KEY_ICMPV6_TYPE_MASK => Self::Icmpv6TypeMask(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ICMPV6_TYPE_MASK))?,
            ),
            TCA_FLOWER_KEY_ARP_SIP => Self::ArpSip(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ARP_SIP))?,
            ),
            TCA_FLOWER_KEY_ARP_SIP_MASK => Self::ArpSipMask(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ARP_SIP_MASK))?,
            ),
            TCA_FLOWER_KEY_ARP_TIP => Self::ArpTip(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ARP_TIP))?,
            ),
            TCA_FLOWER_KEY_ARP_TIP_MASK => Self::ArpTipMask(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ARP_TIP_MASK))?,
            ),
            TCA_FLOWER_KEY_ARP_OP => Self::ArpOp(
                parse_u8(payload).context(nla_err!(TCA_FLOWER_KEY_ARP_OP))?,
            ),
            TCA_FLOWER_KEY_ARP_OP_MASK => Self::ArpOpMask(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ARP_OP_MASK))?,
            ),
            TCA_FLOWER_KEY_ARP_SHA => Self::ArpSha(
                parse_mac(payload).context(nla_err!(TCA_FLOWER_KEY_ARP_SHA))?,
            ),
            TCA_FLOWER_KEY_ARP_SHA_MASK => Self::ArpShaMask(
                parse_mac(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ARP_SHA_MASK))?,
            ),
            TCA_FLOWER_KEY_ARP_THA => Self::ArpTha(
                parse_mac(payload).context(nla_err!(TCA_FLOWER_KEY_ARP_THA))?,
            ),
            TCA_FLOWER_KEY_ARP_THA_MASK => Self::ArpThaMask(
                parse_mac(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ARP_THA_MASK))?,
            ),
            TCA_FLOWER_KEY_MPLS_TTL => Self::MplsTtl(
                parse_u8(payload).context(nla_err!(TCA_FLOWER_KEY_MPLS_TTL))?,
            ),
            TCA_FLOWER_KEY_MPLS_BOS => Self::MplsBos(
                parse_u8(payload).context(nla_err!(TCA_FLOWER_KEY_MPLS_BOS))?,
            ),
            TCA_FLOWER_KEY_MPLS_TC => Self::MplsTc(
                parse_u8(payload).context(nla_err!(TCA_FLOWER_KEY_MPLS_TC))?,
            ),
            TCA_FLOWER_KEY_MPLS_LABEL => Self::MplsLabel(
                parse_u32(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_MPLS_LABEL))?,
            ),
            TCA_FLOWER_KEY_TCP_FLAGS => Self::TcpFlags(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_TCP_FLAGS))?,
            ),
            TCA_FLOWER_KEY_TCP_FLAGS_MASK => Self::TcpFlagsMask(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_TCP_FLAGS_MASK))?,
            ),
            TCA_FLOWER_KEY_FLAGS => Self::KeyFlags(
                parse_u32_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_FLAGS))?,
            ),
            TCA_FLOWER_KEY_FLAGS_MASK => Self::KeyFlagsMask(
                parse_u32_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_FLAGS_MASK))?,
            ),

            TCA_FLOWER_KEY_ENC_IP_TTL => Self::EncKeyIpTtl(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IP_TTL))?,
            ),
            TCA_FLOWER_KEY_ENC_IP_TTL_MASK => Self::EncKeyIpTtlMask(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IP_TTL_MASK))?,
            ),
            TCA_FLOWER_KEY_ENC_IP_TOS => Self::EncKeyIpTos(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IP_TOS))?,
            ),
            TCA_FLOWER_KEY_ENC_IP_TOS_MASK => Self::EncKeyIpTosMask(
                parse_u8(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IP_TOS_MASK))?,
            ),
            TCA_FLOWER_KEY_ENC_IPV4_SRC => Self::EncKeyIpv4Src(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IPV4_SRC))?,
            ),
            TCA_FLOWER_KEY_ENC_IPV4_SRC_MASK => Self::EncKeyIpv4SrcMask(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IPV4_SRC_MASK))?,
            ),
            TCA_FLOWER_KEY_ENC_IPV4_DST => Self::EncKeyIpv4Dst(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IPV4_DST))?,
            ),
            TCA_FLOWER_KEY_ENC_IPV4_DST_MASK => Self::EncKeyIpv4DstMask(
                parse_ipv4_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IPV4_DST_MASK))?,
            ),
            TCA_FLOWER_KEY_ENC_IPV6_SRC => Self::EncKeyIpv6Src(
                parse_ipv6_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IPV6_SRC))?,
            ),
            TCA_FLOWER_KEY_ENC_IPV6_SRC_MASK => Self::EncKeyIpv6SrcMask(
                parse_ipv6_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IPV6_SRC_MASK))?,
            ),
            TCA_FLOWER_KEY_ENC_IPV6_DST => Self::EncKeyIpv6Dst(
                parse_ipv6_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IPV6_DST))?,
            ),
            TCA_FLOWER_KEY_ENC_IPV6_DST_MASK => Self::EncKeyIpv6DstMask(
                parse_ipv6_addr(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_ENC_IPV6_DST_MASK))?,
            ),
            TCA_FLOWER_IN_HW_COUNT => Self::InHwCount(
                parse_u32(payload).context(nla_err!(TCA_FLOWER_IN_HW_COUNT))?,
            ),
            TCA_FLOWER_KEY_PORT_SRC_MIN => Self::PortSrcMin(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_PORT_SRC_MIN))?,
            ),
            TCA_FLOWER_KEY_PORT_SRC_MAX => Self::PortSrcMax(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_PORT_SRC_MAX))?,
            ),
            TCA_FLOWER_KEY_PORT_DST_MIN => Self::PortDstMin(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_PORT_DST_MIN))?,
            ),
            TCA_FLOWER_KEY_PORT_DST_MAX => Self::PortDstMax(
                parse_u16_be(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_PORT_DST_MAX))?,
            ),
            TCA_FLOWER_KEY_CT_STATE => Self::CtState(
                parse_u16(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_CT_STATE))?,
            ),
            TCA_FLOWER_KEY_CT_STATE_MASK => Self::CtStateMask(
                parse_u16(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_CT_STATE_MASK))?,
            ),
            TCA_FLOWER_KEY_CT_ZONE => Self::CtZone(
                parse_u16(payload).context(nla_err!(TCA_FLOWER_KEY_CT_ZONE))?,
            ),
            TCA_FLOWER_KEY_CT_ZONE_MASK => Self::CtZoneMask(
                parse_u16(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_CT_ZONE_MASK))?,
            ),
            TCA_FLOWER_KEY_CT_MARK => Self::CtMark(
                parse_u32(payload).context(nla_err!(TCA_FLOWER_KEY_CT_MARK))?,
            ),
            TCA_FLOWER_KEY_CT_MARK_MASK => Self::CtMarkMask(
                parse_u32(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_CT_MARK_MASK))?,
            ),
            TCA_FLOWER_KEY_CT_LABELS => Self::CtLabels(
                parse_bytes_16(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_CT_LABELS))?,
            ),
            TCA_FLOWER_KEY_CT_LABELS_MASK => Self::CtLabelsMask(
                parse_bytes_16(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_CT_LABELS_MASK))?,
            ),
            TCA_FLOWER_KEY_MPLS_OPTS => {
                let mut nlas = vec![];
                for nla in NlasIterator::new(payload) {
                    let nla =
                        nla.context("invalid TCA_FLOWER_KEY_MPLS_OPTS nla")?;
                    nlas.push(
                        TcFilterFlowerMplsOption::parse(&nla)
                            .context(nla_err!(TCA_FLOWER_KEY_MPLS_OPTS))?,
                    )
                }
                Self::MplsOpts(nlas)
            }
            TCA_FLOWER_KEY_HASH => Self::KeyHash(
                parse_u32(payload).context(nla_err!(TCA_FLOWER_KEY_HASH))?,
            ),
            TCA_FLOWER_KEY_HASH_MASK => Self::KeyHashMask(
                parse_u32(payload)
                    .context(nla_err!(TCA_FLOWER_KEY_HASH_MASK))?,
            ),

            _ => Self::Other(
                DefaultNla::parse(buf).context("failed to parse flower nla")?,
            ),
        })
    }
}
