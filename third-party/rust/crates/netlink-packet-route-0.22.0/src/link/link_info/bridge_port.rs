// SPDX-License-Identifier: MIT
use crate::link::{BridgeId, BridgeIdBuffer};
use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_u16, parse_u32, parse_u64, parse_u8},
    traits::{Emitable, Parseable},
    DecodeError,
};

const IFLA_BRPORT_STATE: u16 = 1;
const IFLA_BRPORT_PRIORITY: u16 = 2;
const IFLA_BRPORT_COST: u16 = 3;
const IFLA_BRPORT_MODE: u16 = 4;
const IFLA_BRPORT_GUARD: u16 = 5;
const IFLA_BRPORT_PROTECT: u16 = 6;
const IFLA_BRPORT_FAST_LEAVE: u16 = 7;
const IFLA_BRPORT_LEARNING: u16 = 8;
const IFLA_BRPORT_UNICAST_FLOOD: u16 = 9;
const IFLA_BRPORT_PROXYARP: u16 = 10;
// only used in one driver, we don't know if its type is stable:
// const IFLA_BRPORT_LEARNING_SYNC: u16 = 11;
const IFLA_BRPORT_PROXYARP_WIFI: u16 = 12;
const IFLA_BRPORT_ROOT_ID: u16 = 13;
const IFLA_BRPORT_BRIDGE_ID: u16 = 14;
const IFLA_BRPORT_DESIGNATED_PORT: u16 = 15;
const IFLA_BRPORT_DESIGNATED_COST: u16 = 16;
const IFLA_BRPORT_ID: u16 = 17;
const IFLA_BRPORT_NO: u16 = 18;
const IFLA_BRPORT_TOPOLOGY_CHANGE_ACK: u16 = 19;
const IFLA_BRPORT_CONFIG_PENDING: u16 = 20;
const IFLA_BRPORT_MESSAGE_AGE_TIMER: u16 = 21;
const IFLA_BRPORT_FORWARD_DELAY_TIMER: u16 = 22;
const IFLA_BRPORT_HOLD_TIMER: u16 = 23;
const IFLA_BRPORT_FLUSH: u16 = 24;
const IFLA_BRPORT_MULTICAST_ROUTER: u16 = 25;
// const IFLA_BRPORT_PAD: u16 = 26;
const IFLA_BRPORT_MCAST_FLOOD: u16 = 27;
const IFLA_BRPORT_MCAST_TO_UCAST: u16 = 28;
const IFLA_BRPORT_VLAN_TUNNEL: u16 = 29;
const IFLA_BRPORT_BCAST_FLOOD: u16 = 30;
const IFLA_BRPORT_GROUP_FWD_MASK: u16 = 31;
const IFLA_BRPORT_NEIGH_SUPPRESS: u16 = 32;
const IFLA_BRPORT_ISOLATED: u16 = 33;
const IFLA_BRPORT_BACKUP_PORT: u16 = 34;
const IFLA_BRPORT_MRP_RING_OPEN: u16 = 35;
const IFLA_BRPORT_MRP_IN_OPEN: u16 = 36;
const IFLA_BRPORT_MCAST_EHT_HOSTS_LIMIT: u16 = 37;
const IFLA_BRPORT_MCAST_EHT_HOSTS_CNT: u16 = 38;
const IFLA_BRPORT_LOCKED: u16 = 39;
const IFLA_BRPORT_MAB: u16 = 40;
const IFLA_BRPORT_MCAST_N_GROUPS: u16 = 41;
const IFLA_BRPORT_MCAST_MAX_GROUPS: u16 = 42;
const IFLA_BRPORT_NEIGH_VLAN_SUPPRESS: u16 = 43;
const IFLA_BRPORT_BACKUP_NHID: u16 = 44;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoBridgePort {
    State(BridgePortState),
    Priority(u16),
    Cost(u32),
    HairpinMode(bool),
    Guard(bool),
    Protect(bool),
    FastLeave(bool),
    Learning(bool),
    UnicastFlood(bool),
    ProxyARP(bool),
    ProxyARPWifi(bool),
    RootId(BridgeId),
    BridgeId(BridgeId),
    DesignatedPort(u16),
    DesignatedCost(u16),
    PortId(u16),
    PortNumber(u16),
    TopologyChangeAck(bool),
    ConfigPending(bool),
    MessageAgeTimer(u64),
    ForwardDelayTimer(u64),
    HoldTimer(u64),
    Flush,
    MulticastRouter(BridgePortMulticastRouter),
    MulticastFlood(bool),
    MulticastToUnicast(bool),
    VlanTunnel(bool),
    BroadcastFlood(bool),
    GroupFwdMask(u16),
    NeighSupress(bool),
    Isolated(bool),
    BackupPort(u32),
    MrpRingOpen(bool),
    MrpInOpen(bool),
    MulticastEhtHostsLimit(u32),
    MulticastEhtHostsCnt(u32),
    Locked(bool),
    Mab(bool),
    MulticastNGroups(u32),
    MulticastMaxGroups(u32),
    NeighVlanSupress(bool),
    BackupNextHopId(u32),
    Other(DefaultNla),
}

impl Nla for InfoBridgePort {
    fn value_len(&self) -> usize {
        match self {
            InfoBridgePort::Flush => 0,
            InfoBridgePort::State(_)
            | InfoBridgePort::HairpinMode(_)
            | InfoBridgePort::Guard(_)
            | InfoBridgePort::Protect(_)
            | InfoBridgePort::FastLeave(_)
            | InfoBridgePort::Learning(_)
            | InfoBridgePort::UnicastFlood(_)
            | InfoBridgePort::ProxyARP(_)
            | InfoBridgePort::ProxyARPWifi(_)
            | InfoBridgePort::TopologyChangeAck(_)
            | InfoBridgePort::ConfigPending(_)
            | InfoBridgePort::MulticastRouter(_)
            | InfoBridgePort::MulticastFlood(_)
            | InfoBridgePort::MulticastToUnicast(_)
            | InfoBridgePort::VlanTunnel(_)
            | InfoBridgePort::BroadcastFlood(_)
            | InfoBridgePort::NeighSupress(_)
            | InfoBridgePort::Isolated(_)
            | InfoBridgePort::MrpRingOpen(_)
            | InfoBridgePort::MrpInOpen(_)
            | InfoBridgePort::Locked(_)
            | InfoBridgePort::Mab(_)
            | InfoBridgePort::NeighVlanSupress(_) => 1,
            InfoBridgePort::Priority(_)
            | InfoBridgePort::DesignatedPort(_)
            | InfoBridgePort::DesignatedCost(_)
            | InfoBridgePort::PortId(_)
            | InfoBridgePort::PortNumber(_)
            | InfoBridgePort::GroupFwdMask(_) => 2,
            InfoBridgePort::Cost(_)
            | InfoBridgePort::BackupPort(_)
            | InfoBridgePort::MulticastEhtHostsLimit(_)
            | InfoBridgePort::MulticastEhtHostsCnt(_)
            | InfoBridgePort::MulticastNGroups(_)
            | InfoBridgePort::MulticastMaxGroups(_)
            | InfoBridgePort::BackupNextHopId(_) => 4,
            InfoBridgePort::RootId(_)
            | InfoBridgePort::BridgeId(_)
            | InfoBridgePort::MessageAgeTimer(_)
            | InfoBridgePort::ForwardDelayTimer(_)
            | InfoBridgePort::HoldTimer(_) => 8,
            InfoBridgePort::Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            InfoBridgePort::Flush => (),
            InfoBridgePort::HairpinMode(value)
            | InfoBridgePort::Guard(value)
            | InfoBridgePort::Protect(value)
            | InfoBridgePort::FastLeave(value)
            | InfoBridgePort::Learning(value)
            | InfoBridgePort::UnicastFlood(value)
            | InfoBridgePort::ProxyARP(value)
            | InfoBridgePort::TopologyChangeAck(value)
            | InfoBridgePort::ConfigPending(value)
            | InfoBridgePort::ProxyARPWifi(value)
            | InfoBridgePort::MulticastFlood(value)
            | InfoBridgePort::MulticastToUnicast(value)
            | InfoBridgePort::VlanTunnel(value)
            | InfoBridgePort::BroadcastFlood(value)
            | InfoBridgePort::NeighSupress(value)
            | InfoBridgePort::Isolated(value)
            | InfoBridgePort::MrpRingOpen(value)
            | InfoBridgePort::MrpInOpen(value)
            | InfoBridgePort::Locked(value)
            | InfoBridgePort::Mab(value)
            | InfoBridgePort::NeighVlanSupress(value) => {
                buffer[0] = if *value { 1 } else { 0 }
            }
            InfoBridgePort::Priority(value)
            | InfoBridgePort::DesignatedPort(value)
            | InfoBridgePort::DesignatedCost(value)
            | InfoBridgePort::PortId(value)
            | InfoBridgePort::PortNumber(value)
            | InfoBridgePort::GroupFwdMask(value) => {
                NativeEndian::write_u16(buffer, *value)
            }
            InfoBridgePort::Cost(value)
            | InfoBridgePort::BackupPort(value)
            | InfoBridgePort::MulticastEhtHostsLimit(value)
            | InfoBridgePort::MulticastEhtHostsCnt(value)
            | InfoBridgePort::MulticastNGroups(value)
            | InfoBridgePort::MulticastMaxGroups(value)
            | InfoBridgePort::BackupNextHopId(value) => {
                NativeEndian::write_u32(buffer, *value)
            }
            InfoBridgePort::MessageAgeTimer(value)
            | InfoBridgePort::ForwardDelayTimer(value)
            | InfoBridgePort::HoldTimer(value) => {
                NativeEndian::write_u64(buffer, *value)
            }
            InfoBridgePort::RootId(bridge_id)
            | InfoBridgePort::BridgeId(bridge_id) => bridge_id.emit(buffer),
            InfoBridgePort::State(state) => buffer[0] = (*state).into(),
            InfoBridgePort::MulticastRouter(mcast_router) => {
                buffer[0] = (*mcast_router).into()
            }
            InfoBridgePort::Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            InfoBridgePort::State(_) => IFLA_BRPORT_STATE,
            InfoBridgePort::Priority(_) => IFLA_BRPORT_PRIORITY,
            InfoBridgePort::Cost(_) => IFLA_BRPORT_COST,
            InfoBridgePort::HairpinMode(_) => IFLA_BRPORT_MODE,
            InfoBridgePort::Guard(_) => IFLA_BRPORT_GUARD,
            InfoBridgePort::Protect(_) => IFLA_BRPORT_PROTECT,
            InfoBridgePort::FastLeave(_) => IFLA_BRPORT_FAST_LEAVE,
            InfoBridgePort::Learning(_) => IFLA_BRPORT_LEARNING,
            InfoBridgePort::UnicastFlood(_) => IFLA_BRPORT_UNICAST_FLOOD,
            InfoBridgePort::ProxyARP(_) => IFLA_BRPORT_PROXYARP,
            InfoBridgePort::ProxyARPWifi(_) => IFLA_BRPORT_PROXYARP_WIFI,
            InfoBridgePort::RootId(_) => IFLA_BRPORT_ROOT_ID,
            InfoBridgePort::BridgeId(_) => IFLA_BRPORT_BRIDGE_ID,
            InfoBridgePort::DesignatedPort(_) => IFLA_BRPORT_DESIGNATED_PORT,
            InfoBridgePort::DesignatedCost(_) => IFLA_BRPORT_DESIGNATED_COST,
            InfoBridgePort::PortId(_) => IFLA_BRPORT_ID,
            InfoBridgePort::PortNumber(_) => IFLA_BRPORT_NO,
            InfoBridgePort::TopologyChangeAck(_) => {
                IFLA_BRPORT_TOPOLOGY_CHANGE_ACK
            }
            InfoBridgePort::ConfigPending(_) => IFLA_BRPORT_CONFIG_PENDING,
            InfoBridgePort::MessageAgeTimer(_) => IFLA_BRPORT_MESSAGE_AGE_TIMER,
            InfoBridgePort::ForwardDelayTimer(_) => {
                IFLA_BRPORT_FORWARD_DELAY_TIMER
            }
            InfoBridgePort::HoldTimer(_) => IFLA_BRPORT_HOLD_TIMER,
            InfoBridgePort::Flush => IFLA_BRPORT_FLUSH,
            InfoBridgePort::MulticastRouter(_) => IFLA_BRPORT_MULTICAST_ROUTER,
            InfoBridgePort::MulticastFlood(_) => IFLA_BRPORT_MCAST_FLOOD,
            InfoBridgePort::MulticastToUnicast(_) => IFLA_BRPORT_MCAST_TO_UCAST,
            InfoBridgePort::VlanTunnel(_) => IFLA_BRPORT_VLAN_TUNNEL,
            InfoBridgePort::BroadcastFlood(_) => IFLA_BRPORT_BCAST_FLOOD,
            InfoBridgePort::GroupFwdMask(_) => IFLA_BRPORT_GROUP_FWD_MASK,
            InfoBridgePort::NeighSupress(_) => IFLA_BRPORT_NEIGH_SUPPRESS,
            InfoBridgePort::Isolated(_) => IFLA_BRPORT_ISOLATED,
            InfoBridgePort::BackupPort(_) => IFLA_BRPORT_BACKUP_PORT,
            InfoBridgePort::MrpRingOpen(_) => IFLA_BRPORT_MRP_RING_OPEN,
            InfoBridgePort::MrpInOpen(_) => IFLA_BRPORT_MRP_IN_OPEN,
            InfoBridgePort::MulticastEhtHostsLimit(_) => {
                IFLA_BRPORT_MCAST_EHT_HOSTS_LIMIT
            }
            InfoBridgePort::MulticastEhtHostsCnt(_) => {
                IFLA_BRPORT_MCAST_EHT_HOSTS_CNT
            }
            InfoBridgePort::Locked(_) => IFLA_BRPORT_LOCKED,
            InfoBridgePort::Mab(_) => IFLA_BRPORT_MAB,
            InfoBridgePort::MulticastNGroups(_) => IFLA_BRPORT_MCAST_N_GROUPS,
            InfoBridgePort::MulticastMaxGroups(_) => {
                IFLA_BRPORT_MCAST_MAX_GROUPS
            }
            InfoBridgePort::NeighVlanSupress(_) => {
                IFLA_BRPORT_NEIGH_VLAN_SUPPRESS
            }
            InfoBridgePort::BackupNextHopId(_) => IFLA_BRPORT_BACKUP_NHID,
            InfoBridgePort::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for InfoBridgePort
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();

        Ok(match buf.kind() {
            IFLA_BRPORT_STATE => InfoBridgePort::State(
                parse_u8(payload)
                    .with_context(|| {
                        format!("invalid IFLA_BRPORT_STATE {payload:?}")
                    })?
                    .into(),
            ),
            IFLA_BRPORT_PRIORITY => {
                InfoBridgePort::Priority(parse_u16(payload).with_context(
                    || format!("invalid IFLA_BRPORT_PRIORITY {payload:?}"),
                )?)
            }
            IFLA_BRPORT_COST => {
                InfoBridgePort::Cost(parse_u32(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_COST {payload:?}")
                })?)
            }
            IFLA_BRPORT_MODE => InfoBridgePort::HairpinMode(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_MODE {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_GUARD => InfoBridgePort::Guard(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_GUARD {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_PROTECT => InfoBridgePort::Protect(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_PROTECT {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_FAST_LEAVE => InfoBridgePort::FastLeave(
                parse_u8(payload).with_context(|| {
                    format!(
                        "invalid IFLA_BRPORT_FAST_LEAVE {payload:?}"
                    )
                })? > 0,
            ),
            IFLA_BRPORT_LEARNING => InfoBridgePort::Learning(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_LEARNING {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_UNICAST_FLOOD => InfoBridgePort::UnicastFlood(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_UNICAST_FLOOD {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_PROXYARP => InfoBridgePort::ProxyARP(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_PROXYARP {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_PROXYARP_WIFI => InfoBridgePort::ProxyARPWifi(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_PROXYARP_WIFI {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_ROOT_ID => Self::RootId(
                BridgeId::parse(&BridgeIdBuffer::new(payload)).with_context(|| {
                    format!("invalid IFLA_BRPORT_ROOT_ID {payload:?}")
                })?,
            ),
            IFLA_BRPORT_BRIDGE_ID => Self::BridgeId(
                BridgeId::parse(&BridgeIdBuffer::new(payload)).with_context(|| {
                    format!("invalid IFLA_BRPORT_BRIDGE_ID {payload:?}")
                })?,
            ),
            IFLA_BRPORT_DESIGNATED_PORT => InfoBridgePort::DesignatedPort(
                parse_u16(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_DESIGNATED_PORT {payload:?}")
                })?,
            ),
            IFLA_BRPORT_DESIGNATED_COST => InfoBridgePort::DesignatedCost(
                parse_u16(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_DESIGNATED_COST {payload:?}")
                })?,
            ),
            IFLA_BRPORT_ID => {
                InfoBridgePort::PortId(parse_u16(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_ID {payload:?}")
                })?)
            }
            IFLA_BRPORT_NO => {
                InfoBridgePort::PortNumber(parse_u16(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_NO {payload:?}")
                })?)
            }
            IFLA_BRPORT_TOPOLOGY_CHANGE_ACK => {
                InfoBridgePort::TopologyChangeAck(
                    parse_u8(payload).with_context(|| {
                        format!(
                            "invalid IFLA_BRPORT_TOPOLOGY_CHANGE_ACK {payload:?}"
                        )
                    })? > 0,
                )
            }
            IFLA_BRPORT_CONFIG_PENDING => InfoBridgePort::ConfigPending(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_CONFIG_PENDING {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_MESSAGE_AGE_TIMER => InfoBridgePort::MessageAgeTimer(
                parse_u64(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_MESSAGE_AGE_TIMER {payload:?}")
                })?,
            ),
            IFLA_BRPORT_FORWARD_DELAY_TIMER => {
                InfoBridgePort::ForwardDelayTimer(
                    parse_u64(payload).with_context(|| {
                        format!(
                            "invalid IFLA_BRPORT_FORWARD_DELAY_TIMER {payload:?}"
                        )
                    })?,
                )
            }
            IFLA_BRPORT_HOLD_TIMER => {
                InfoBridgePort::HoldTimer(parse_u64(payload).with_context(
                    || format!("invalid IFLA_BRPORT_HOLD_TIMER {payload:?}"),
                )?)
            }
            IFLA_BRPORT_FLUSH => InfoBridgePort::Flush,
            IFLA_BRPORT_MULTICAST_ROUTER => InfoBridgePort::MulticastRouter(
                parse_u8(payload)
                    .with_context(|| {
                        format!(
                            "invalid IFLA_BRPORT_MULTICAST_ROUTER {payload:?}"
                        )
                    })?
                    .into(),
            ),
            IFLA_BRPORT_MCAST_FLOOD => InfoBridgePort::MulticastFlood(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_MCAST_FLOOD {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_MCAST_TO_UCAST => InfoBridgePort::MulticastToUnicast(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_MCAST_TO_UCAST {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_VLAN_TUNNEL => InfoBridgePort::VlanTunnel(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_VLAN_TUNNEL {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_BCAST_FLOOD => InfoBridgePort::BroadcastFlood(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_BCAST_FLOOD {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_GROUP_FWD_MASK => InfoBridgePort::GroupFwdMask(
                parse_u16(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_GROUP_FWD_MASK {payload:?}")
                })?,
            ),
            IFLA_BRPORT_NEIGH_SUPPRESS => InfoBridgePort::NeighSupress(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_NEIGH_SUPPRESS {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_ISOLATED => InfoBridgePort::Isolated(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_ISOLATED {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_BACKUP_PORT => {
                InfoBridgePort::BackupPort(parse_u32(payload).with_context(
                    || format!("invalid IFLA_BRPORT_BACKUP_PORT {payload:?}"),
                )?)
            }
            IFLA_BRPORT_MRP_RING_OPEN => InfoBridgePort::MrpRingOpen(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_MRP_RING_OPEN {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_MRP_IN_OPEN => InfoBridgePort::MrpInOpen(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_MRP_IN_OPEN {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_MCAST_EHT_HOSTS_LIMIT => {
                InfoBridgePort::MulticastEhtHostsLimit(
                    parse_u32(payload).with_context(|| {
                        format!(
                            "invalid IFLA_BRPORT_MCAST_EHT_HOSTS_LIMIT {payload:?}"
                        )
                    })?,
                )
            }
            IFLA_BRPORT_MCAST_EHT_HOSTS_CNT => {
                InfoBridgePort::MulticastEhtHostsCnt(
                    parse_u32(payload).with_context(|| {
                        format!(
                            "invalid IFLA_BRPORT_MCAST_EHT_HOSTS_CNT {payload:?}"
                        )
                    })?
                )
            }
            IFLA_BRPORT_LOCKED => InfoBridgePort::Locked(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_LOCKED {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_MAB => InfoBridgePort::Mab(
                parse_u8(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_MAB {payload:?}")
                })? > 0,
            ),
            IFLA_BRPORT_MCAST_N_GROUPS => InfoBridgePort::MulticastNGroups(
                parse_u32(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_MCAST_N_GROUPS {payload:?}")
                })?,
            ),
            IFLA_BRPORT_MCAST_MAX_GROUPS => InfoBridgePort::MulticastMaxGroups(
                parse_u32(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_MCAST_MAX_GROUPS {payload:?}")
                })?,
            ),
            IFLA_BRPORT_NEIGH_VLAN_SUPPRESS => InfoBridgePort::NeighVlanSupress(
                parse_u8(payload).with_context(|| {
                    format!(
                        "invalid IFLA_BRPORT_NEIGH_VLAN_SUPPRESS {payload:?}"
                    )
                })? > 0,
            ),
            IFLA_BRPORT_BACKUP_NHID => InfoBridgePort::BackupNextHopId(
                parse_u32(payload).with_context(|| {
                    format!("invalid IFLA_BRPORT_BACKUP_NHID {payload:?}")
                })?,
            ),
            kind => InfoBridgePort::Other(
                DefaultNla::parse(buf).with_context(|| {
                    format!(
                        "failed to parse bridge port NLA of type '{kind}' into DefaultNla"
                    )
                })?,
            ),
        })
    }
}

const BR_STATE_DISABLED: u8 = 0;
const BR_STATE_LISTENING: u8 = 1;
const BR_STATE_LEARNING: u8 = 2;
const BR_STATE_FORWARDING: u8 = 3;
const BR_STATE_BLOCKING: u8 = 4;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum BridgePortState {
    Disabled,
    Listening,
    Learning,
    Forwarding,
    Blocking,
    Other(u8),
}

impl From<u8> for BridgePortState {
    fn from(value: u8) -> Self {
        match value {
            BR_STATE_DISABLED => BridgePortState::Disabled,
            BR_STATE_LISTENING => BridgePortState::Listening,
            BR_STATE_LEARNING => BridgePortState::Learning,
            BR_STATE_FORWARDING => BridgePortState::Forwarding,
            BR_STATE_BLOCKING => BridgePortState::Blocking,
            _ => BridgePortState::Other(value),
        }
    }
}

impl From<BridgePortState> for u8 {
    fn from(value: BridgePortState) -> Self {
        match value {
            BridgePortState::Disabled => BR_STATE_DISABLED,
            BridgePortState::Listening => BR_STATE_LISTENING,
            BridgePortState::Learning => BR_STATE_LEARNING,
            BridgePortState::Forwarding => BR_STATE_FORWARDING,
            BridgePortState::Blocking => BR_STATE_BLOCKING,
            BridgePortState::Other(v) => v,
        }
    }
}

const MDB_RTR_TYPE_DISABLED: u8 = 0;
const MDB_RTR_TYPE_TEMP_QUERY: u8 = 1;
const MDB_RTR_TYPE_PERM: u8 = 2;
const MDB_RTR_TYPE_TEMP: u8 = 3;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum BridgePortMulticastRouter {
    Disabled,
    TempQuery,
    Perm,
    Temp,
    Other(u8),
}

impl From<u8> for BridgePortMulticastRouter {
    fn from(value: u8) -> Self {
        match value {
            MDB_RTR_TYPE_DISABLED => BridgePortMulticastRouter::Disabled,
            MDB_RTR_TYPE_TEMP_QUERY => BridgePortMulticastRouter::TempQuery,
            MDB_RTR_TYPE_PERM => BridgePortMulticastRouter::Perm,
            MDB_RTR_TYPE_TEMP => BridgePortMulticastRouter::Temp,
            _ => BridgePortMulticastRouter::Other(value),
        }
    }
}

impl From<BridgePortMulticastRouter> for u8 {
    fn from(value: BridgePortMulticastRouter) -> Self {
        match value {
            BridgePortMulticastRouter::Disabled => MDB_RTR_TYPE_DISABLED,
            BridgePortMulticastRouter::TempQuery => MDB_RTR_TYPE_TEMP_QUERY,
            BridgePortMulticastRouter::Perm => MDB_RTR_TYPE_PERM,
            BridgePortMulticastRouter::Temp => MDB_RTR_TYPE_TEMP,
            BridgePortMulticastRouter::Other(v) => v,
        }
    }
}
