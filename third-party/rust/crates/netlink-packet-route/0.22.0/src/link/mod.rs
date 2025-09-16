// SPDX-License-Identifier: MIT

mod af_spec;
mod attribute;
mod buffer_tool;
mod down_reason;
mod event;
pub(crate) mod ext_mask;
mod header;
mod link_flag;
mod link_info;
mod link_layer_type;
mod link_state;
mod map;
mod message;
mod phys_id;
mod prop_list;
mod proto_info;
pub(crate) mod sriov;
mod stats;
mod stats64;
mod vlan_protocol;
mod wireless;
mod xdp;

mod tests;

pub use self::af_spec::{
    AfSpecBridge, AfSpecInet, AfSpecInet6, AfSpecUnspec, BridgeFlag,
    BridgeMode, BridgeVlanInfo, BridgeVlanInfoFlags, BridgeVlanTunnelInfo,
    Icmp6Stats, Icmp6StatsBuffer, Inet6CacheInfo, Inet6CacheInfoBuffer,
    Inet6DevConf, Inet6DevConfBuffer, Inet6IfaceFlags, Inet6Stats,
    Inet6StatsBuffer, InetDevConf,
};
pub use self::attribute::LinkAttribute;
pub use self::down_reason::LinkProtocolDownReason;
pub use self::event::LinkEvent;
pub use self::ext_mask::LinkExtentMask;
pub use self::header::{LinkHeader, LinkMessageBuffer};
pub use self::link_flag::LinkFlags;
pub use self::link_info::{
    BondAdInfo, BondArpValidate, BondMode, BondPortState, BridgeId,
    BridgeIdBuffer, BridgePortMulticastRouter, BridgePortState,
    BridgeQuerierState, GeneveDf, HsrProtocol, InfoBond, InfoBondPort,
    InfoBridge, InfoBridgePort, InfoData, InfoGeneve, InfoGreTap, InfoGreTap6,
    InfoGreTun, InfoGreTun6, InfoGtp, InfoHsr, InfoIpVlan, InfoIpVtap,
    InfoIpoib, InfoKind, InfoMacSec, InfoMacVlan, InfoMacVtap, InfoPortData,
    InfoPortKind, InfoSitTun, InfoTun, InfoVeth, InfoVlan, InfoVrf,
    InfoVrfPort, InfoVti, InfoVxlan, InfoXfrm, IpVlanFlags, IpVlanMode,
    IpVtapFlags, IpVtapMode, LinkInfo, LinkXstats, MacSecCipherId,
    MacSecOffload, MacSecValidate, MacVlanMode, MacVtapMode, MiiStatus,
    VlanQosMapping,
};
pub use self::link_layer_type::LinkLayerType;
pub use self::link_state::State;
pub use self::map::{Map, MapBuffer};
pub use self::message::LinkMessage;
pub use self::phys_id::LinkPhysId;
pub use self::prop_list::Prop;
pub use self::proto_info::{LinkProtoInfoBridge, LinkProtoInfoInet6};
pub use self::sriov::{
    LinkVfInfo, LinkVfPort, VfInfo, VfInfoBroadcast, VfInfoBroadcastBuffer,
    VfInfoGuid, VfInfoGuidBuffer, VfInfoLinkState, VfInfoLinkStateBuffer,
    VfInfoMac, VfInfoMacBuffer, VfInfoRate, VfInfoRateBuffer, VfInfoRssQueryEn,
    VfInfoRssQueryEnBuffer, VfInfoSpoofCheck, VfInfoSpoofCheckBuffer,
    VfInfoTrust, VfInfoTrustBuffer, VfInfoTxRate, VfInfoTxRateBuffer,
    VfInfoVlan, VfInfoVlanBuffer, VfLinkState, VfPort, VfStats, VfVlan,
    VfVlanInfo,
};
pub use self::stats::{Stats, StatsBuffer};
pub use self::stats64::{Stats64, Stats64Buffer};
pub use self::vlan_protocol::VlanProtocol;
pub use self::wireless::LinkWirelessEvent;
pub use self::xdp::{LinkXdp, XdpAttached};
