// SPDX-License-Identifier: MIT

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use anyhow::Context;
use byteorder::{BigEndian, ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator, NLA_F_NESTED},
    parsers::{
        parse_ip, parse_mac, parse_u16, parse_u16_be, parse_u32, parse_u64,
        parse_u8,
    },
    traits::{Emitable, Parseable},
    DecodeError,
};

const IFLA_BR_FORWARD_DELAY: u16 = 1;
const IFLA_BR_HELLO_TIME: u16 = 2;
const IFLA_BR_MAX_AGE: u16 = 3;
const IFLA_BR_AGEING_TIME: u16 = 4;
const IFLA_BR_STP_STATE: u16 = 5;
const IFLA_BR_PRIORITY: u16 = 6;
const IFLA_BR_VLAN_FILTERING: u16 = 7;
const IFLA_BR_VLAN_PROTOCOL: u16 = 8;
const IFLA_BR_GROUP_FWD_MASK: u16 = 9;
const IFLA_BR_ROOT_ID: u16 = 10;
const IFLA_BR_BRIDGE_ID: u16 = 11;
const IFLA_BR_ROOT_PORT: u16 = 12;
const IFLA_BR_ROOT_PATH_COST: u16 = 13;
const IFLA_BR_TOPOLOGY_CHANGE: u16 = 14;
const IFLA_BR_TOPOLOGY_CHANGE_DETECTED: u16 = 15;
const IFLA_BR_HELLO_TIMER: u16 = 16;
const IFLA_BR_TCN_TIMER: u16 = 17;
const IFLA_BR_TOPOLOGY_CHANGE_TIMER: u16 = 18;
const IFLA_BR_GC_TIMER: u16 = 19;
const IFLA_BR_GROUP_ADDR: u16 = 20;
const IFLA_BR_FDB_FLUSH: u16 = 21;
const IFLA_BR_MCAST_ROUTER: u16 = 22;
const IFLA_BR_MCAST_SNOOPING: u16 = 23;
const IFLA_BR_MCAST_QUERY_USE_IFADDR: u16 = 24;
const IFLA_BR_MCAST_QUERIER: u16 = 25;
const IFLA_BR_MCAST_HASH_ELASTICITY: u16 = 26;
const IFLA_BR_MCAST_HASH_MAX: u16 = 27;
const IFLA_BR_MCAST_LAST_MEMBER_CNT: u16 = 28;
const IFLA_BR_MCAST_STARTUP_QUERY_CNT: u16 = 29;
const IFLA_BR_MCAST_LAST_MEMBER_INTVL: u16 = 30;
const IFLA_BR_MCAST_MEMBERSHIP_INTVL: u16 = 31;
const IFLA_BR_MCAST_QUERIER_INTVL: u16 = 32;
const IFLA_BR_MCAST_QUERY_INTVL: u16 = 33;
const IFLA_BR_MCAST_QUERY_RESPONSE_INTVL: u16 = 34;
const IFLA_BR_MCAST_STARTUP_QUERY_INTVL: u16 = 35;
const IFLA_BR_NF_CALL_IPTABLES: u16 = 36;
const IFLA_BR_NF_CALL_IP6TABLES: u16 = 37;
const IFLA_BR_NF_CALL_ARPTABLES: u16 = 38;
const IFLA_BR_VLAN_DEFAULT_PVID: u16 = 39;
// const IFLA_BR_PAD: u16 = 40;
const IFLA_BR_VLAN_STATS_ENABLED: u16 = 41;
const IFLA_BR_MCAST_STATS_ENABLED: u16 = 42;
const IFLA_BR_MCAST_IGMP_VERSION: u16 = 43;
const IFLA_BR_MCAST_MLD_VERSION: u16 = 44;
const IFLA_BR_VLAN_STATS_PER_PORT: u16 = 45;
const IFLA_BR_MULTI_BOOLOPT: u16 = 46;
const IFLA_BR_MCAST_QUERIER_STATE: u16 = 47;

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoBridge {
    GroupAddr([u8; 6]),
    FdbFlush,
    HelloTimer(u64),
    TcnTimer(u64),
    TopologyChangeTimer(u64),
    GcTimer(u64),
    MulticastMembershipInterval(u64),
    MulticastQuerierInterval(u64),
    MulticastQueryInterval(u64),
    MulticastQueryResponseInterval(u64),
    MulticastLastMemberInterval(u64),
    MulticastStartupQueryInterval(u64),
    ForwardDelay(u32),
    HelloTime(u32),
    MaxAge(u32),
    AgeingTime(u32),
    StpState(u32),
    MulticastHashElasticity(u32),
    MulticastHashMax(u32),
    MulticastLastMemberCount(u32),
    MulticastStartupQueryCount(u32),
    RootPathCost(u32),
    Priority(u16),
    VlanProtocol(u16),
    GroupFwdMask(u16),
    RootId(BridgeId),
    BridgeId(BridgeId),
    RootPort(u16),
    VlanDefaultPvid(u16),
    VlanFiltering(bool),
    TopologyChange(u8),
    TopologyChangeDetected(u8),
    MulticastRouter(u8),
    MulticastSnooping(u8),
    MulticastQueryUseIfaddr(u8),
    MulticastQuerier(u8),
    NfCallIpTables(u8),
    NfCallIp6Tables(u8),
    NfCallArpTables(u8),
    VlanStatsEnabled(u8),
    MulticastStatsEnabled(u8),
    MulticastIgmpVersion(u8),
    MulticastMldVersion(u8),
    VlanStatsPerHost(u8),
    MultiBoolOpt(u64),
    MulticastQuerierState(Vec<BridgeQuerierState>),
    Other(DefaultNla),
}

impl Nla for InfoBridge {
    fn value_len(&self) -> usize {
        match self {
            // The existance of FdbFlush means true
            Self::FdbFlush => 0,
            Self::HelloTimer(_)
            | Self::TcnTimer(_)
            | Self::TopologyChangeTimer(_)
            | Self::GcTimer(_)
            | Self::MulticastMembershipInterval(_)
            | Self::MulticastQuerierInterval(_)
            | Self::MulticastQueryInterval(_)
            | Self::MulticastQueryResponseInterval(_)
            | Self::MulticastLastMemberInterval(_)
            | Self::MulticastStartupQueryInterval(_) => 8,
            Self::ForwardDelay(_)
            | Self::HelloTime(_)
            | Self::MaxAge(_)
            | Self::AgeingTime(_)
            | Self::StpState(_)
            | Self::MulticastHashElasticity(_)
            | Self::MulticastHashMax(_)
            | Self::MulticastLastMemberCount(_)
            | Self::MulticastStartupQueryCount(_)
            | Self::RootPathCost(_) => 4,
            Self::Priority(_)
            | Self::VlanProtocol(_)
            | Self::GroupFwdMask(_)
            | Self::RootPort(_)
            | Self::VlanDefaultPvid(_) => 2,

            Self::RootId(_) | Self::BridgeId(_) | Self::MultiBoolOpt(_) => 8,

            Self::GroupAddr(_) => 6,

            Self::VlanFiltering(_) => 1,
            Self::TopologyChange(_)
            | Self::TopologyChangeDetected(_)
            | Self::MulticastRouter(_)
            | Self::MulticastSnooping(_)
            | Self::MulticastQueryUseIfaddr(_)
            | Self::MulticastQuerier(_)
            | Self::NfCallIpTables(_)
            | Self::NfCallIp6Tables(_)
            | Self::NfCallArpTables(_)
            | Self::VlanStatsEnabled(_)
            | Self::MulticastStatsEnabled(_)
            | Self::MulticastIgmpVersion(_)
            | Self::MulticastMldVersion(_)
            | Self::VlanStatsPerHost(_) => 1,

            Self::MulticastQuerierState(nlas) => nlas.as_slice().buffer_len(),

            Self::Other(nla) => nla.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::FdbFlush => (),

            Self::HelloTimer(value)
            | Self::TcnTimer(value)
            | Self::TopologyChangeTimer(value)
            | Self::GcTimer(value)
            | Self::MulticastMembershipInterval(value)
            | Self::MulticastQuerierInterval(value)
            | Self::MulticastQueryInterval(value)
            | Self::MulticastQueryResponseInterval(value)
            | Self::MulticastLastMemberInterval(value)
            | Self::MulticastStartupQueryInterval(value)
            | Self::MultiBoolOpt(value) => {
                NativeEndian::write_u64(buffer, *value)
            }

            Self::ForwardDelay(value)
            | Self::HelloTime(value)
            | Self::MaxAge(value)
            | Self::AgeingTime(value)
            | Self::StpState(value)
            | Self::MulticastHashElasticity(value)
            | Self::MulticastHashMax(value)
            | Self::MulticastLastMemberCount(value)
            | Self::MulticastStartupQueryCount(value)
            | Self::RootPathCost(value) => {
                NativeEndian::write_u32(buffer, *value)
            }

            Self::Priority(value)
            | Self::GroupFwdMask(value)
            | Self::RootPort(value)
            | Self::VlanDefaultPvid(value) => {
                NativeEndian::write_u16(buffer, *value)
            }

            Self::VlanProtocol(value) => BigEndian::write_u16(buffer, *value),

            Self::RootId(bridge_id) | Self::BridgeId(bridge_id) => {
                bridge_id.emit(buffer)
            }

            Self::GroupAddr(value) => buffer.copy_from_slice(&value[..]),

            Self::VlanFiltering(value) => buffer[0] = (*value).into(),
            Self::TopologyChange(value)
            | Self::TopologyChangeDetected(value)
            | Self::MulticastRouter(value)
            | Self::MulticastSnooping(value)
            | Self::MulticastQueryUseIfaddr(value)
            | Self::MulticastQuerier(value)
            | Self::NfCallIpTables(value)
            | Self::NfCallIp6Tables(value)
            | Self::NfCallArpTables(value)
            | Self::VlanStatsEnabled(value)
            | Self::MulticastStatsEnabled(value)
            | Self::MulticastIgmpVersion(value)
            | Self::MulticastMldVersion(value)
            | Self::VlanStatsPerHost(value) => buffer[0] = *value,

            Self::MulticastQuerierState(nlas) => nlas.as_slice().emit(buffer),

            Self::Other(nla) => nla.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::GroupAddr(_) => IFLA_BR_GROUP_ADDR,
            Self::FdbFlush => IFLA_BR_FDB_FLUSH,
            Self::HelloTimer(_) => IFLA_BR_HELLO_TIMER,
            Self::TcnTimer(_) => IFLA_BR_TCN_TIMER,
            Self::TopologyChangeTimer(_) => IFLA_BR_TOPOLOGY_CHANGE_TIMER,
            Self::GcTimer(_) => IFLA_BR_GC_TIMER,
            Self::MulticastMembershipInterval(_) => {
                IFLA_BR_MCAST_MEMBERSHIP_INTVL
            }
            Self::MulticastQuerierInterval(_) => IFLA_BR_MCAST_QUERIER_INTVL,
            Self::MulticastQueryInterval(_) => IFLA_BR_MCAST_QUERY_INTVL,
            Self::MulticastQueryResponseInterval(_) => {
                IFLA_BR_MCAST_QUERY_RESPONSE_INTVL
            }
            Self::ForwardDelay(_) => IFLA_BR_FORWARD_DELAY,
            Self::HelloTime(_) => IFLA_BR_HELLO_TIME,
            Self::MaxAge(_) => IFLA_BR_MAX_AGE,
            Self::AgeingTime(_) => IFLA_BR_AGEING_TIME,
            Self::StpState(_) => IFLA_BR_STP_STATE,
            Self::MulticastHashElasticity(_) => IFLA_BR_MCAST_HASH_ELASTICITY,
            Self::MulticastHashMax(_) => IFLA_BR_MCAST_HASH_MAX,
            Self::MulticastLastMemberCount(_) => IFLA_BR_MCAST_LAST_MEMBER_CNT,
            Self::MulticastStartupQueryCount(_) => {
                IFLA_BR_MCAST_STARTUP_QUERY_CNT
            }
            Self::MulticastLastMemberInterval(_) => {
                IFLA_BR_MCAST_LAST_MEMBER_INTVL
            }
            Self::MulticastStartupQueryInterval(_) => {
                IFLA_BR_MCAST_STARTUP_QUERY_INTVL
            }
            Self::RootPathCost(_) => IFLA_BR_ROOT_PATH_COST,
            Self::Priority(_) => IFLA_BR_PRIORITY,
            Self::VlanProtocol(_) => IFLA_BR_VLAN_PROTOCOL,
            Self::GroupFwdMask(_) => IFLA_BR_GROUP_FWD_MASK,
            Self::RootId(_) => IFLA_BR_ROOT_ID,
            Self::BridgeId(_) => IFLA_BR_BRIDGE_ID,
            Self::RootPort(_) => IFLA_BR_ROOT_PORT,
            Self::VlanDefaultPvid(_) => IFLA_BR_VLAN_DEFAULT_PVID,
            Self::VlanFiltering(_) => IFLA_BR_VLAN_FILTERING,
            Self::TopologyChange(_) => IFLA_BR_TOPOLOGY_CHANGE,
            Self::TopologyChangeDetected(_) => IFLA_BR_TOPOLOGY_CHANGE_DETECTED,
            Self::MulticastRouter(_) => IFLA_BR_MCAST_ROUTER,
            Self::MulticastSnooping(_) => IFLA_BR_MCAST_SNOOPING,
            Self::MulticastQueryUseIfaddr(_) => IFLA_BR_MCAST_QUERY_USE_IFADDR,
            Self::MulticastQuerier(_) => IFLA_BR_MCAST_QUERIER,
            Self::NfCallIpTables(_) => IFLA_BR_NF_CALL_IPTABLES,
            Self::NfCallIp6Tables(_) => IFLA_BR_NF_CALL_IP6TABLES,
            Self::NfCallArpTables(_) => IFLA_BR_NF_CALL_ARPTABLES,
            Self::VlanStatsEnabled(_) => IFLA_BR_VLAN_STATS_ENABLED,
            Self::MulticastStatsEnabled(_) => IFLA_BR_MCAST_STATS_ENABLED,
            Self::MulticastIgmpVersion(_) => IFLA_BR_MCAST_IGMP_VERSION,
            Self::MulticastMldVersion(_) => IFLA_BR_MCAST_MLD_VERSION,
            Self::VlanStatsPerHost(_) => IFLA_BR_VLAN_STATS_PER_PORT,
            Self::MultiBoolOpt(_) => IFLA_BR_MULTI_BOOLOPT,
            Self::MulticastQuerierState(_) => {
                IFLA_BR_MCAST_QUERIER_STATE | NLA_F_NESTED
            }
            Self::Other(nla) => nla.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoBridge {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_BR_FDB_FLUSH => Self::FdbFlush,
            IFLA_BR_HELLO_TIMER => Self::HelloTimer(
                parse_u64(payload)
                    .context("invalid IFLA_BR_HELLO_TIMER value")?,
            ),
            IFLA_BR_TCN_TIMER => Self::TcnTimer(
                parse_u64(payload)
                    .context("invalid IFLA_BR_TCN_TIMER value")?,
            ),
            IFLA_BR_TOPOLOGY_CHANGE_TIMER => Self::TopologyChangeTimer(
                parse_u64(payload)
                    .context("invalid IFLA_BR_TOPOLOGY_CHANGE_TIMER value")?,
            ),
            IFLA_BR_GC_TIMER => Self::GcTimer(
                parse_u64(payload).context("invalid IFLA_BR_GC_TIMER value")?,
            ),
            IFLA_BR_MCAST_LAST_MEMBER_INTVL => {
                Self::MulticastLastMemberInterval(
                    parse_u64(payload).context(
                        "invalid IFLA_BR_MCAST_LAST_MEMBER_INTVL value",
                    )?,
                )
            }
            IFLA_BR_MCAST_MEMBERSHIP_INTVL => {
                Self::MulticastMembershipInterval(
                    parse_u64(payload).context(
                        "invalid IFLA_BR_MCAST_MEMBERSHIP_INTVL value",
                    )?,
                )
            }
            IFLA_BR_MCAST_QUERIER_INTVL => Self::MulticastQuerierInterval(
                parse_u64(payload)
                    .context("invalid IFLA_BR_MCAST_QUERIER_INTVL value")?,
            ),
            IFLA_BR_MCAST_QUERY_INTVL => Self::MulticastQueryInterval(
                parse_u64(payload)
                    .context("invalid IFLA_BR_MCAST_QUERY_INTVL value")?,
            ),
            IFLA_BR_MCAST_QUERY_RESPONSE_INTVL => {
                Self::MulticastQueryResponseInterval(
                    parse_u64(payload).context(
                        "invalid IFLA_BR_MCAST_QUERY_RESPONSE_INTVL value",
                    )?,
                )
            }
            IFLA_BR_MCAST_STARTUP_QUERY_INTVL => {
                Self::MulticastStartupQueryInterval(
                    parse_u64(payload).context(
                        "invalid IFLA_BR_MCAST_STARTUP_QUERY_INTVL value",
                    )?,
                )
            }
            IFLA_BR_FORWARD_DELAY => Self::ForwardDelay(
                parse_u32(payload)
                    .context("invalid IFLA_BR_FORWARD_DELAY value")?,
            ),
            IFLA_BR_HELLO_TIME => Self::HelloTime(
                parse_u32(payload)
                    .context("invalid IFLA_BR_HELLO_TIME value")?,
            ),
            IFLA_BR_MAX_AGE => Self::MaxAge(
                parse_u32(payload).context("invalid IFLA_BR_MAX_AGE value")?,
            ),
            IFLA_BR_AGEING_TIME => Self::AgeingTime(
                parse_u32(payload)
                    .context("invalid IFLA_BR_AGEING_TIME value")?,
            ),
            IFLA_BR_STP_STATE => Self::StpState(
                parse_u32(payload)
                    .context("invalid IFLA_BR_STP_STATE value")?,
            ),
            IFLA_BR_MCAST_HASH_ELASTICITY => Self::MulticastHashElasticity(
                parse_u32(payload)
                    .context("invalid IFLA_BR_MCAST_HASH_ELASTICITY value")?,
            ),
            IFLA_BR_MCAST_HASH_MAX => Self::MulticastHashMax(
                parse_u32(payload)
                    .context("invalid IFLA_BR_MCAST_HASH_MAX value")?,
            ),
            IFLA_BR_MCAST_LAST_MEMBER_CNT => Self::MulticastLastMemberCount(
                parse_u32(payload)
                    .context("invalid IFLA_BR_MCAST_LAST_MEMBER_CNT value")?,
            ),
            IFLA_BR_MCAST_STARTUP_QUERY_CNT => {
                Self::MulticastStartupQueryCount(
                    parse_u32(payload).context(
                        "invalid IFLA_BR_MCAST_STARTUP_QUERY_CNT value",
                    )?,
                )
            }
            IFLA_BR_ROOT_PATH_COST => Self::RootPathCost(
                parse_u32(payload)
                    .context("invalid IFLA_BR_ROOT_PATH_COST value")?,
            ),
            IFLA_BR_PRIORITY => Self::Priority(
                parse_u16(payload).context("invalid IFLA_BR_PRIORITY value")?,
            ),
            IFLA_BR_VLAN_PROTOCOL => Self::VlanProtocol(
                parse_u16_be(payload)
                    .context("invalid IFLA_BR_VLAN_PROTOCOL value")?,
            ),
            IFLA_BR_GROUP_FWD_MASK => Self::GroupFwdMask(
                parse_u16(payload)
                    .context("invalid IFLA_BR_GROUP_FWD_MASK value")?,
            ),
            IFLA_BR_ROOT_ID => Self::RootId(
                BridgeId::parse(&BridgeIdBuffer::new(payload))
                    .context("invalid IFLA_BR_ROOT_ID value")?,
            ),
            IFLA_BR_BRIDGE_ID => Self::BridgeId(
                BridgeId::parse(&BridgeIdBuffer::new(payload))
                    .context("invalid IFLA_BR_BRIDGE_ID value")?,
            ),
            IFLA_BR_GROUP_ADDR => Self::GroupAddr(
                parse_mac(payload)
                    .context("invalid IFLA_BR_GROUP_ADDR value")?,
            ),
            IFLA_BR_ROOT_PORT => Self::RootPort(
                parse_u16(payload)
                    .context("invalid IFLA_BR_ROOT_PORT value")?,
            ),
            IFLA_BR_VLAN_DEFAULT_PVID => Self::VlanDefaultPvid(
                parse_u16(payload)
                    .context("invalid IFLA_BR_VLAN_DEFAULT_PVID value")?,
            ),
            IFLA_BR_VLAN_FILTERING => Self::VlanFiltering(
                parse_u8(payload)
                    .context("invalid IFLA_BR_VLAN_FILTERING value")?
                    > 0,
            ),
            IFLA_BR_TOPOLOGY_CHANGE => Self::TopologyChange(
                parse_u8(payload)
                    .context("invalid IFLA_BR_TOPOLOGY_CHANGE value")?,
            ),
            IFLA_BR_TOPOLOGY_CHANGE_DETECTED => {
                Self::TopologyChangeDetected(parse_u8(payload).context(
                    "invalid IFLA_BR_TOPOLOGY_CHANGE_DETECTED value",
                )?)
            }
            IFLA_BR_MCAST_ROUTER => Self::MulticastRouter(
                parse_u8(payload)
                    .context("invalid IFLA_BR_MCAST_ROUTER value")?,
            ),
            IFLA_BR_MCAST_SNOOPING => Self::MulticastSnooping(
                parse_u8(payload)
                    .context("invalid IFLA_BR_MCAST_SNOOPING value")?,
            ),
            IFLA_BR_MCAST_QUERY_USE_IFADDR => Self::MulticastQueryUseIfaddr(
                parse_u8(payload)
                    .context("invalid IFLA_BR_MCAST_QUERY_USE_IFADDR value")?,
            ),
            IFLA_BR_MCAST_QUERIER => Self::MulticastQuerier(
                parse_u8(payload)
                    .context("invalid IFLA_BR_MCAST_QUERIER value")?,
            ),
            IFLA_BR_NF_CALL_IPTABLES => Self::NfCallIpTables(
                parse_u8(payload)
                    .context("invalid IFLA_BR_NF_CALL_IPTABLES value")?,
            ),
            IFLA_BR_NF_CALL_IP6TABLES => Self::NfCallIp6Tables(
                parse_u8(payload)
                    .context("invalid IFLA_BR_NF_CALL_IP6TABLES value")?,
            ),
            IFLA_BR_NF_CALL_ARPTABLES => Self::NfCallArpTables(
                parse_u8(payload)
                    .context("invalid IFLA_BR_NF_CALL_ARPTABLES value")?,
            ),
            IFLA_BR_VLAN_STATS_ENABLED => Self::VlanStatsEnabled(
                parse_u8(payload)
                    .context("invalid IFLA_BR_VLAN_STATS_ENABLED value")?,
            ),
            IFLA_BR_MCAST_STATS_ENABLED => Self::MulticastStatsEnabled(
                parse_u8(payload)
                    .context("invalid IFLA_BR_MCAST_STATS_ENABLED value")?,
            ),
            IFLA_BR_MCAST_IGMP_VERSION => Self::MulticastIgmpVersion(
                parse_u8(payload)
                    .context("invalid IFLA_BR_MCAST_IGMP_VERSION value")?,
            ),
            IFLA_BR_MCAST_MLD_VERSION => Self::MulticastMldVersion(
                parse_u8(payload)
                    .context("invalid IFLA_BR_MCAST_MLD_VERSION value")?,
            ),
            IFLA_BR_VLAN_STATS_PER_PORT => Self::VlanStatsPerHost(
                parse_u8(payload)
                    .context("invalid IFLA_BR_VLAN_STATS_PER_PORT value")?,
            ),
            IFLA_BR_MULTI_BOOLOPT => Self::MultiBoolOpt(
                parse_u64(payload)
                    .context("invalid IFLA_BR_MULTI_BOOLOPT value")?,
            ),
            IFLA_BR_MCAST_QUERIER_STATE => {
                let mut v = Vec::new();
                let err = "failed to parse IFLA_BR_MCAST_QUERIER_STATE";
                for nla in NlasIterator::new(payload) {
                    let nla = &nla.context(err)?;
                    let parsed = BridgeQuerierState::parse(nla).context(err)?;
                    v.push(parsed);
                }
                Self::MulticastQuerierState(v)
            }
            _ => Self::Other(DefaultNla::parse(buf).context(
                "invalid link info bridge NLA value (unknown type)",
            )?),
        })
    }
}

const BRIDGE_ID_LEN: usize = 8;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BridgeId {
    pub priority: u16,
    pub address: [u8; 6],
}

buffer!(BridgeIdBuffer(BRIDGE_ID_LEN) {
    priority: (u16, 0..2),
    address: (slice, 2..BRIDGE_ID_LEN)
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<BridgeIdBuffer<&T>> for BridgeId {
    fn parse(buf: &BridgeIdBuffer<&T>) -> Result<Self, DecodeError> {
        // Priority is encoded in big endian. From kernel's
        // net/bridge/br_netlink.c br_fill_info():
        // u16 priority = (br->bridge_id.prio[0] << 8) | br->bridge_id.prio[1];
        Ok(Self {
            priority: u16::from_be(buf.priority()),
            address: parse_mac(buf.address())
                .context("invalid MAC address in BridgeId buffer")?,
        })
    }
}

impl Emitable for BridgeId {
    fn buffer_len(&self) -> usize {
        BRIDGE_ID_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = BridgeIdBuffer::new(buffer);
        buffer.set_priority(self.priority.to_be());
        buffer.address_mut().copy_from_slice(&self.address[..]);
    }
}

const BRIDGE_QUERIER_IP_ADDRESS: u16 = 1;
const BRIDGE_QUERIER_IP_PORT: u16 = 2;
const BRIDGE_QUERIER_IP_OTHER_TIMER: u16 = 3;
// const BRIDGE_QUERIER_PAD: u16 = 4;
const BRIDGE_QUERIER_IPV6_ADDRESS: u16 = 5;
const BRIDGE_QUERIER_IPV6_PORT: u16 = 6;
const BRIDGE_QUERIER_IPV6_OTHER_TIMER: u16 = 7;

#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum BridgeQuerierState {
    Ipv4Address(Ipv4Addr),
    Ipv4Port(u32),
    Ipv4OtherTimer(u64),
    Ipv6Address(Ipv6Addr),
    Ipv6Port(u32),
    Ipv6OtherTimer(u64),
    Other(DefaultNla),
}

impl Nla for BridgeQuerierState {
    fn value_len(&self) -> usize {
        use self::BridgeQuerierState::*;
        match self {
            Ipv4Address(_) => 4,
            Ipv6Address(_) => 16,
            Ipv4Port(_) | Ipv6Port(_) => 4,
            Ipv4OtherTimer(_) | Ipv6OtherTimer(_) => 8,
            Other(nla) => nla.value_len(),
        }
    }

    fn kind(&self) -> u16 {
        use self::BridgeQuerierState::*;
        match self {
            Ipv4Address(_) => BRIDGE_QUERIER_IP_ADDRESS,
            Ipv4Port(_) => BRIDGE_QUERIER_IP_PORT,
            Ipv4OtherTimer(_) => BRIDGE_QUERIER_IP_OTHER_TIMER,
            Ipv6Address(_) => BRIDGE_QUERIER_IPV6_ADDRESS,
            Ipv6Port(_) => BRIDGE_QUERIER_IPV6_PORT,
            Ipv6OtherTimer(_) => BRIDGE_QUERIER_IPV6_OTHER_TIMER,
            Other(nla) => nla.kind(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        use self::BridgeQuerierState::*;
        match self {
            Ipv4Port(d) | Ipv6Port(d) => NativeEndian::write_u32(buffer, *d),
            Ipv4OtherTimer(d) | Ipv6OtherTimer(d) => {
                NativeEndian::write_u64(buffer, *d)
            }
            Ipv4Address(addr) => buffer.copy_from_slice(&addr.octets()),
            Ipv6Address(addr) => buffer.copy_from_slice(&addr.octets()),
            Other(nla) => nla.emit_value(buffer),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>>
    for BridgeQuerierState
{
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        use self::BridgeQuerierState::*;
        let payload = buf.value();
        Ok(match buf.kind() {
            BRIDGE_QUERIER_IP_ADDRESS => match parse_ip(payload) {
                Ok(IpAddr::V4(addr)) => Ipv4Address(addr),
                Ok(v) => {
                    return Err(DecodeError::from(format!(
                        "Invalid BRIDGE_QUERIER_IP_ADDRESS, \
                        expecting IPv4 address, but got {v}"
                    )))
                }
                Err(e) => {
                    return Err(DecodeError::from(format!(
                        "Invalid BRIDGE_QUERIER_IP_ADDRESS {e}"
                    )))
                }
            },
            BRIDGE_QUERIER_IPV6_ADDRESS => match parse_ip(payload) {
                Ok(IpAddr::V6(addr)) => Ipv6Address(addr),
                Ok(v) => {
                    return Err(DecodeError::from(format!(
                        "Invalid BRIDGE_QUERIER_IPV6_ADDRESS, \
                        expecting IPv6 address, but got {v}"
                    )));
                }
                Err(e) => {
                    return Err(DecodeError::from(format!(
                        "Invalid BRIDGE_QUERIER_IPV6_ADDRESS {e}"
                    )));
                }
            },
            BRIDGE_QUERIER_IP_PORT => Ipv4Port(
                parse_u32(payload)
                    .context("invalid BRIDGE_QUERIER_IP_PORT value")?,
            ),
            BRIDGE_QUERIER_IPV6_PORT => Ipv6Port(
                parse_u32(payload)
                    .context("invalid BRIDGE_QUERIER_IPV6_PORT value")?,
            ),
            BRIDGE_QUERIER_IP_OTHER_TIMER => Ipv4OtherTimer(
                parse_u64(payload)
                    .context("invalid BRIDGE_QUERIER_IP_OTHER_TIMER value")?,
            ),
            BRIDGE_QUERIER_IPV6_OTHER_TIMER => Ipv6OtherTimer(
                parse_u64(payload)
                    .context("invalid BRIDGE_QUERIER_IPV6_OTHER_TIMER value")?,
            ),

            kind => Other(
                DefaultNla::parse(buf)
                    .context(format!("unknown NLA type {kind}"))?,
            ),
        })
    }
}
