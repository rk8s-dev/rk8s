// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    parsers::parse_string,
    DecodeError, Emitable, Parseable, ParseableParametrized,
};

use super::super::{InfoData, InfoPortData, InfoPortKind, LinkXstats};

const IFLA_INFO_KIND: u16 = 1;
const IFLA_INFO_DATA: u16 = 2;
const IFLA_INFO_XSTATS: u16 = 3;
const IFLA_INFO_PORT_KIND: u16 = 4;
const IFLA_INFO_PORT_DATA: u16 = 5;

const DUMMY: &str = "dummy";
const IFB: &str = "ifb";
const BRIDGE: &str = "bridge";
const TUN: &str = "tun";
const NLMON: &str = "nlmon";
const VLAN: &str = "vlan";
const VETH: &str = "veth";
const VXLAN: &str = "vxlan";
const BOND: &str = "bond";
const IPVLAN: &str = "ipvlan";
const IPVTAP: &str = "ipvtap";
const MACVLAN: &str = "macvlan";
const MACVTAP: &str = "macvtap";
const GRETAP: &str = "gretap";
const IP6GRETAP: &str = "ip6gretap";
const IPIP: &str = "ipip";
const SIT: &str = "sit";
const GRE: &str = "gre";
const IP6GRE: &str = "ip6gre";
const VTI: &str = "vti";
const VRF: &str = "vrf";
const GTP: &str = "gtp";
const IPOIB: &str = "ipoib";
const WIREGUARD: &str = "wireguard";
const XFRM: &str = "xfrm";
const MACSEC: &str = "macsec";
const HSR: &str = "hsr";
const GENEVE: &str = "geneve";

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum LinkInfo {
    Xstats(LinkXstats),
    Kind(InfoKind),
    Data(InfoData),
    PortKind(InfoPortKind),
    PortData(InfoPortData),
    Other(DefaultNla),
}

impl Nla for LinkInfo {
    fn value_len(&self) -> usize {
        match self {
            Self::Xstats(v) => v.buffer_len(),
            Self::Kind(nla) => nla.value_len(),
            Self::Data(nla) => nla.value_len(),
            Self::PortKind(nla) => nla.value_len(),
            Self::PortData(nla) => nla.value_len(),
            Self::Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Xstats(v) => v.emit(buffer),
            Self::Kind(nla) => nla.emit_value(buffer),
            Self::Data(nla) => nla.emit_value(buffer),
            Self::PortKind(nla) => nla.emit_value(buffer),
            Self::PortData(nla) => nla.emit_value(buffer),
            Self::Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Xstats(_) => IFLA_INFO_XSTATS,
            Self::PortKind(_) => IFLA_INFO_PORT_KIND,
            Self::PortData(_) => IFLA_INFO_PORT_DATA,
            Self::Kind(_) => IFLA_INFO_KIND,
            Self::Data(_) => IFLA_INFO_DATA,
            Self::Other(nla) => nla.kind(),
        }
    }
}

pub(crate) struct VecLinkInfo(pub(crate) Vec<LinkInfo>);

// We cannot `impl Parseable<_> for Info` because some attributes
// depend on each other. To parse IFLA_INFO_DATA we first need to
// parse the preceding IFLA_INFO_KIND for example.
//
// Moreover, with cannot `impl Parseable for Vec<LinkInfo>` due to the
// orphan rule: `Parseable` and `Vec<_>` are both defined outside of
// this crate. Thus, we create this internal VecLinkInfo struct that wraps
// `Vec<LinkInfo>` and allows us to circumvent the orphan rule.
//
// The downside is that this impl will not be exposed.

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for VecLinkInfo {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let mut nlas = Vec::new();
        let mut link_info_kind: Option<InfoKind> = None;
        let mut link_info_port_kind: Option<InfoPortKind> = None;
        for nla in NlasIterator::new(buf.into_inner()) {
            let nla = nla?;
            match nla.kind() {
                IFLA_INFO_XSTATS => {
                    if let Some(link_info_kind) = &link_info_kind {
                        nlas.push(LinkInfo::Xstats(
                            LinkXstats::parse_with_param(&nla, link_info_kind)?,
                        ));
                    } else {
                        return Err("IFLA_INFO_XSTATS is not \
                            preceded by an IFLA_INFO_KIND"
                            .into());
                    }
                }
                IFLA_INFO_PORT_KIND => {
                    let parsed = InfoPortKind::parse(&nla)?;
                    nlas.push(LinkInfo::PortKind(parsed.clone()));
                    link_info_port_kind = Some(parsed);
                }
                IFLA_INFO_PORT_DATA => {
                    if let Some(link_info_port_kind) = link_info_port_kind {
                        nlas.push(LinkInfo::PortData(
                            InfoPortData::parse_with_param(
                                nla.value(),
                                link_info_port_kind,
                            )?,
                        ));
                    } else {
                        return Err("IFLA_INFO_PORT_DATA is not preceded by \
                            an IFLA_INFO_PORT_KIND"
                            .into());
                    }
                    link_info_port_kind = None;
                }
                IFLA_INFO_KIND => {
                    let parsed = InfoKind::parse(&nla)?;
                    nlas.push(LinkInfo::Kind(parsed.clone()));
                    link_info_kind = Some(parsed);
                }
                IFLA_INFO_DATA => {
                    if let Some(link_info_kind) = &link_info_kind {
                        nlas.push(LinkInfo::Data(InfoData::parse_with_param(
                            nla.value(),
                            link_info_kind,
                        )?));
                    } else {
                        return Err("IFLA_INFO_DATA is not preceded by an \
                            IFLA_INFO_KIND"
                            .into());
                    }
                }
                _kind => nlas.push(LinkInfo::Other(
                    DefaultNla::parse(&nla).context(format!(
                        "Unknown NLA type for IFLA_INFO_DATA {:?}",
                        nla
                    ))?,
                )),
            }
        }
        Ok(Self(nlas))
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoKind {
    Dummy,
    Ifb,
    Bridge,
    Tun,
    Nlmon,
    Vlan,
    Veth,
    Vxlan,
    Bond,
    IpVlan,
    IpVtap,
    MacVlan,
    MacVtap,
    GreTap,
    GreTap6,
    IpTun,
    SitTun,
    GreTun,
    GreTun6,
    Vti,
    Vrf,
    Gtp,
    Ipoib,
    Wireguard,
    Xfrm,
    MacSec,
    Hsr,
    Geneve,
    Other(String),
}

impl std::fmt::Display for InfoKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Dummy => DUMMY,
                Self::Ifb => IFB,
                Self::Bridge => BRIDGE,
                Self::Tun => TUN,
                Self::Nlmon => NLMON,
                Self::Vlan => VLAN,
                Self::Veth => VETH,
                Self::Vxlan => VXLAN,
                Self::Bond => BOND,
                Self::IpVlan => IPVLAN,
                Self::IpVtap => IPVTAP,
                Self::MacVlan => MACVLAN,
                Self::MacVtap => MACVTAP,
                Self::GreTap => GRETAP,
                Self::GreTap6 => IP6GRETAP,
                Self::IpTun => IPIP,
                Self::SitTun => SIT,
                Self::GreTun => GRE,
                Self::GreTun6 => IP6GRE,
                Self::Vti => VTI,
                Self::Vrf => VRF,
                Self::Gtp => GTP,
                Self::Ipoib => IPOIB,
                Self::Wireguard => WIREGUARD,
                Self::Xfrm => XFRM,
                Self::MacSec => MACSEC,
                Self::Hsr => HSR,
                Self::Geneve => GENEVE,
                Self::Other(s) => s.as_str(),
            }
        )
    }
}

impl Nla for InfoKind {
    fn value_len(&self) -> usize {
        let len = match self {
            Self::Dummy => DUMMY.len(),
            Self::Ifb => IFB.len(),
            Self::Bridge => BRIDGE.len(),
            Self::Tun => TUN.len(),
            Self::Nlmon => NLMON.len(),
            Self::Vlan => VLAN.len(),
            Self::Veth => VETH.len(),
            Self::Vxlan => VXLAN.len(),
            Self::Bond => BOND.len(),
            Self::IpVlan => IPVLAN.len(),
            Self::IpVtap => IPVTAP.len(),
            Self::MacVlan => MACVLAN.len(),
            Self::MacVtap => MACVTAP.len(),
            Self::GreTap => GRETAP.len(),
            Self::GreTap6 => IP6GRETAP.len(),
            Self::IpTun => IPIP.len(),
            Self::SitTun => SIT.len(),
            Self::GreTun => GRE.len(),
            Self::GreTun6 => IP6GRE.len(),
            Self::Vti => VTI.len(),
            Self::Vrf => VRF.len(),
            Self::Gtp => GTP.len(),
            Self::Ipoib => IPOIB.len(),
            Self::Wireguard => WIREGUARD.len(),
            Self::Xfrm => XFRM.len(),
            Self::MacSec => MACSEC.len(),
            Self::Hsr => HSR.len(),
            Self::Geneve => GENEVE.len(),
            Self::Other(s) => s.len(),
        };
        len + 1
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        let kind = self.to_string();
        let s = kind.as_str();
        buffer[..s.len()].copy_from_slice(s.to_string().as_bytes());
        buffer[s.len()] = 0;
    }

    fn kind(&self) -> u16 {
        IFLA_INFO_KIND
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoKind {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<InfoKind, DecodeError> {
        if buf.kind() != IFLA_INFO_KIND {
            return Err(format!(
                "failed to parse IFLA_INFO_KIND: NLA type is {}",
                buf.kind()
            )
            .into());
        }
        let s = parse_string(buf.value())
            .context("invalid IFLA_INFO_KIND value")?;
        Ok(match s.as_str() {
            DUMMY => Self::Dummy,
            IFB => Self::Ifb,
            BRIDGE => Self::Bridge,
            TUN => Self::Tun,
            NLMON => Self::Nlmon,
            VLAN => Self::Vlan,
            VETH => Self::Veth,
            VXLAN => Self::Vxlan,
            BOND => Self::Bond,
            IPVLAN => Self::IpVlan,
            IPVTAP => Self::IpVtap,
            MACVLAN => Self::MacVlan,
            MACVTAP => Self::MacVtap,
            GRETAP => Self::GreTap,
            IP6GRETAP => Self::GreTap6,
            IPIP => Self::IpTun,
            SIT => Self::SitTun,
            GRE => Self::GreTun,
            IP6GRE => Self::GreTun6,
            VTI => Self::Vti,
            VRF => Self::Vrf,
            GTP => Self::Gtp,
            IPOIB => Self::Ipoib,
            WIREGUARD => Self::Wireguard,
            MACSEC => Self::MacSec,
            XFRM => Self::Xfrm,
            HSR => Self::Hsr,
            GENEVE => Self::Geneve,
            _ => Self::Other(s),
        })
    }
}
