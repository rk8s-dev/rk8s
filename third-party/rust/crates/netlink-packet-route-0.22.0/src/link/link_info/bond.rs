// SPDX-License-Identifier: MIT

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    ops::Deref,
};

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer, NlasIterator},
    parsers::{parse_ip, parse_mac, parse_u16, parse_u32, parse_u8},
    traits::{Emitable, Parseable},
    DecodeError,
};

const IFLA_BOND_AD_INFO_AGGREGATOR: u16 = 1;
const IFLA_BOND_AD_INFO_NUM_PORTS: u16 = 2;
const IFLA_BOND_AD_INFO_ACTOR_KEY: u16 = 3;
const IFLA_BOND_AD_INFO_PARTNER_KEY: u16 = 4;
const IFLA_BOND_AD_INFO_PARTNER_MAC: u16 = 5;

const IFLA_BOND_MODE: u16 = 1;
const IFLA_BOND_ACTIVE_PORT: u16 = 2;
const IFLA_BOND_MIIMON: u16 = 3;
const IFLA_BOND_UPDELAY: u16 = 4;
const IFLA_BOND_DOWNDELAY: u16 = 5;
const IFLA_BOND_USE_CARRIER: u16 = 6;
const IFLA_BOND_ARP_INTERVAL: u16 = 7;
const IFLA_BOND_ARP_IP_TARGET: u16 = 8;
const IFLA_BOND_ARP_VALIDATE: u16 = 9;
const IFLA_BOND_ARP_ALL_TARGETS: u16 = 10;
const IFLA_BOND_PRIMARY: u16 = 11;
const IFLA_BOND_PRIMARY_RESELECT: u16 = 12;
const IFLA_BOND_FAIL_OVER_MAC: u16 = 13;
const IFLA_BOND_XMIT_HASH_POLICY: u16 = 14;
const IFLA_BOND_RESEND_IGMP: u16 = 15;
const IFLA_BOND_NUM_PEER_NOTIF: u16 = 16;
const IFLA_BOND_ALL_PORTS_ACTIVE: u16 = 17;
const IFLA_BOND_MIN_LINKS: u16 = 18;
const IFLA_BOND_LP_INTERVAL: u16 = 19;
const IFLA_BOND_PACKETS_PER_PORT: u16 = 20;
const IFLA_BOND_AD_LACP_RATE: u16 = 21;
const IFLA_BOND_AD_SELECT: u16 = 22;
const IFLA_BOND_AD_INFO: u16 = 23;
const IFLA_BOND_AD_ACTOR_SYS_PRIO: u16 = 24;
const IFLA_BOND_AD_USER_PORT_KEY: u16 = 25;
const IFLA_BOND_AD_ACTOR_SYSTEM: u16 = 26;
const IFLA_BOND_TLB_DYNAMIC_LB: u16 = 27;
const IFLA_BOND_PEER_NOTIF_DELAY: u16 = 28;
const IFLA_BOND_AD_LACP_ACTIVE: u16 = 29;
const IFLA_BOND_MISSED_MAX: u16 = 30;
const IFLA_BOND_NS_IP6_TARGET: u16 = 31;

const BOND_MODE_ROUNDROBIN: u8 = 0;
const BOND_MODE_ACTIVEBACKUP: u8 = 1;
const BOND_MODE_XOR: u8 = 2;
const BOND_MODE_BROADCAST: u8 = 3;
const BOND_MODE_8023AD: u8 = 4;
const BOND_MODE_TLB: u8 = 5;
const BOND_MODE_ALB: u8 = 6;

const BOND_STATE_ACTIVE: u8 = 0;
const BOND_STATE_BACKUP: u8 = 1;

const BOND_ARP_VALIDATE_NONE: u32 = 0;
const BOND_ARP_VALIDATE_ACTIVE: u32 = 1 << BOND_STATE_ACTIVE as u32;
const BOND_ARP_VALIDATE_BACKUP: u32 = 1 << BOND_STATE_BACKUP as u32;
const BOND_ARP_VALIDATE_ALL: u32 =
    BOND_ARP_VALIDATE_ACTIVE | BOND_ARP_VALIDATE_BACKUP;
const BOND_ARP_FILTER: u32 = BOND_ARP_VALIDATE_ALL + 1;
const BOND_ARP_FILTER_ACTIVE: u32 = BOND_ARP_FILTER | BOND_ARP_VALIDATE_ACTIVE;
const BOND_ARP_FILTER_BACKUP: u32 = BOND_ARP_FILTER | BOND_ARP_VALIDATE_BACKUP;

#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum BondAdInfo {
    Aggregator(u16),
    NumPorts(u16),
    ActorKey(u16),
    PartnerKey(u16),
    PartnerMac([u8; 6]),
    Other(DefaultNla),
}

impl Nla for BondAdInfo {
    fn value_len(&self) -> usize {
        match self {
            Self::Aggregator(_)
            | Self::NumPorts(_)
            | Self::ActorKey(_)
            | Self::PartnerKey(_) => 2,
            Self::PartnerMac(_) => 6,
            Self::Other(v) => v.value_len(),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Aggregator(_) => IFLA_BOND_AD_INFO_AGGREGATOR,
            Self::NumPorts(_) => IFLA_BOND_AD_INFO_NUM_PORTS,
            Self::ActorKey(_) => IFLA_BOND_AD_INFO_ACTOR_KEY,
            Self::PartnerKey(_) => IFLA_BOND_AD_INFO_PARTNER_KEY,
            Self::PartnerMac(_) => IFLA_BOND_AD_INFO_PARTNER_MAC,
            Self::Other(v) => v.kind(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Aggregator(d)
            | Self::NumPorts(d)
            | Self::ActorKey(d)
            | Self::PartnerKey(d) => NativeEndian::write_u16(buffer, *d),
            Self::PartnerMac(mac) => buffer.copy_from_slice(mac),
            Self::Other(v) => v.emit_value(buffer),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for BondAdInfo {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_BOND_AD_INFO_AGGREGATOR => Self::Aggregator(
                parse_u16(payload)
                    .context("invalid IFLA_BOND_AD_INFO_AGGREGATOR value")?,
            ),
            IFLA_BOND_AD_INFO_NUM_PORTS => Self::NumPorts(
                parse_u16(payload)
                    .context("invalid IFLA_BOND_AD_INFO_NUM_PORTS value")?,
            ),
            IFLA_BOND_AD_INFO_ACTOR_KEY => Self::ActorKey(
                parse_u16(payload)
                    .context("invalid IFLA_BOND_AD_INFO_ACTOR_KEY value")?,
            ),
            IFLA_BOND_AD_INFO_PARTNER_KEY => Self::PartnerKey(
                parse_u16(payload)
                    .context("invalid IFLA_BOND_AD_INFO_PARTNER_KEY value")?,
            ),
            IFLA_BOND_AD_INFO_PARTNER_MAC => Self::PartnerMac(
                parse_mac(payload)
                    .context("invalid IFLA_BOND_AD_INFO_PARTNER_MAC value")?,
            ),
            _ => Self::Other(DefaultNla::parse(buf).context(format!(
                "invalid NLA for {}: {payload:?}",
                buf.kind()
            ))?),
        })
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub enum BondMode {
    #[default]
    BalanceRr,
    ActiveBackup,
    BalanceXor,
    Broadcast,
    Ieee8023Ad,
    BalanceTlb,
    BalanceAlb,
    Other(u8),
}

impl From<u8> for BondMode {
    fn from(d: u8) -> Self {
        match d {
            BOND_MODE_ROUNDROBIN => Self::BalanceRr,
            BOND_MODE_ACTIVEBACKUP => Self::ActiveBackup,
            BOND_MODE_XOR => Self::BalanceXor,
            BOND_MODE_BROADCAST => Self::Broadcast,
            BOND_MODE_8023AD => Self::Ieee8023Ad,
            BOND_MODE_TLB => Self::BalanceTlb,
            BOND_MODE_ALB => Self::BalanceAlb,
            _ => Self::Other(d),
        }
    }
}

impl From<BondMode> for u8 {
    fn from(d: BondMode) -> Self {
        match d {
            BondMode::BalanceRr => BOND_MODE_ROUNDROBIN,
            BondMode::ActiveBackup => BOND_MODE_ACTIVEBACKUP,
            BondMode::BalanceXor => BOND_MODE_XOR,
            BondMode::Broadcast => BOND_MODE_BROADCAST,
            BondMode::Ieee8023Ad => BOND_MODE_8023AD,
            BondMode::BalanceTlb => BOND_MODE_TLB,
            BondMode::BalanceAlb => BOND_MODE_ALB,
            BondMode::Other(d) => d,
        }
    }
}

impl std::fmt::Display for BondMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kernel_name = match self {
            BondMode::BalanceRr => "balance-rr",
            BondMode::ActiveBackup => "active-backup",
            BondMode::BalanceXor => "balance-xor",
            BondMode::Broadcast => "broadcast",
            BondMode::Ieee8023Ad => "802.3ad",
            BondMode::BalanceTlb => "balance-tlb",
            BondMode::BalanceAlb => "balance-alb",
            BondMode::Other(d) => return write!(f, "unknown-variant ({d})"),
        };

        f.write_str(kernel_name)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum BondArpValidate {
    #[default]
    None,
    Active,
    Backup,
    All,
    Filter,
    FilterActive,
    FilterBackup,
    Other(u32),
}

impl From<BondArpValidate> for u32 {
    fn from(value: BondArpValidate) -> Self {
        match value {
            BondArpValidate::None => BOND_ARP_VALIDATE_NONE,
            BondArpValidate::Active => BOND_ARP_VALIDATE_ACTIVE,
            BondArpValidate::Backup => BOND_ARP_VALIDATE_BACKUP,
            BondArpValidate::All => BOND_ARP_VALIDATE_ALL,
            BondArpValidate::Filter => BOND_ARP_FILTER,
            BondArpValidate::FilterActive => BOND_ARP_FILTER_ACTIVE,
            BondArpValidate::FilterBackup => BOND_ARP_FILTER_BACKUP,
            BondArpValidate::Other(d) => d,
        }
    }
}

impl From<u32> for BondArpValidate {
    fn from(value: u32) -> Self {
        match value {
            BOND_ARP_VALIDATE_NONE => BondArpValidate::None,
            BOND_ARP_VALIDATE_ACTIVE => BondArpValidate::Active,
            BOND_ARP_VALIDATE_BACKUP => BondArpValidate::Backup,
            BOND_ARP_VALIDATE_ALL => BondArpValidate::All,
            BOND_ARP_FILTER => BondArpValidate::Filter,
            BOND_ARP_FILTER_ACTIVE => BondArpValidate::FilterActive,
            BOND_ARP_FILTER_BACKUP => BondArpValidate::FilterBackup,
            d => BondArpValidate::Other(d),
        }
    }
}

impl std::fmt::Display for BondArpValidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kernel_name = match self {
            BondArpValidate::None => "none",
            BondArpValidate::Active => "active",
            BondArpValidate::Backup => "backup",
            BondArpValidate::All => "all",
            BondArpValidate::Filter => "filter",
            BondArpValidate::FilterActive => "filter_active",
            BondArpValidate::FilterBackup => "filter_backup",
            BondArpValidate::Other(d) => {
                return write!(f, "unknown-variant ({d})")
            }
        };
        f.write_str(kernel_name)
    }
}

// Some attributes (ARP_IP_TARGET, NS_IP6_TARGET) contain a nested
// list of IP addresses, where each element uses the index as NLA kind
// and the address as value. InfoBond exposes vectors of IP addresses,
// and we use this struct for serialization.
struct BondIpAddrNla {
    index: u16,
    addr: IpAddr,
}

struct BondIpAddrNlaList(Vec<BondIpAddrNla>);

impl Deref for BondIpAddrNlaList {
    type Target = Vec<BondIpAddrNla>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&Vec<Ipv4Addr>> for BondIpAddrNlaList {
    fn from(addrs: &Vec<Ipv4Addr>) -> Self {
        let mut nlas = Vec::new();
        for (i, addr) in addrs.iter().enumerate() {
            let nla = BondIpAddrNla {
                index: i as u16,
                addr: IpAddr::V4(*addr),
            };
            nlas.push(nla);
        }
        BondIpAddrNlaList(nlas)
    }
}

impl From<&Vec<Ipv6Addr>> for BondIpAddrNlaList {
    fn from(addrs: &Vec<Ipv6Addr>) -> Self {
        let mut nlas = Vec::new();
        for (i, addr) in addrs.iter().enumerate() {
            let nla = BondIpAddrNla {
                index: i as u16,
                addr: IpAddr::V6(*addr),
            };
            nlas.push(nla);
        }
        BondIpAddrNlaList(nlas)
    }
}

impl Nla for BondIpAddrNla {
    fn value_len(&self) -> usize {
        if self.addr.is_ipv4() {
            4
        } else {
            16
        }
    }
    fn emit_value(&self, buffer: &mut [u8]) {
        match self.addr {
            IpAddr::V4(addr) => buffer.copy_from_slice(&addr.octets()),
            IpAddr::V6(addr) => buffer.copy_from_slice(&addr.octets()),
        }
    }
    fn kind(&self) -> u16 {
        self.index
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum InfoBond {
    Mode(BondMode),
    ActivePort(u32),
    MiiMon(u32),
    UpDelay(u32),
    DownDelay(u32),
    UseCarrier(u8),
    ArpInterval(u32),
    ArpIpTarget(Vec<Ipv4Addr>),
    ArpValidate(BondArpValidate),
    ArpAllTargets(u32),
    Primary(u32),
    PrimaryReselect(u8),
    FailOverMac(u8),
    XmitHashPolicy(u8),
    ResendIgmp(u32),
    NumPeerNotif(u8),
    AllPortsActive(u8),
    MinLinks(u32),
    LpInterval(u32),
    PacketsPerPort(u32),
    AdLacpRate(u8),
    AdSelect(u8),
    AdInfo(Vec<BondAdInfo>),
    AdActorSysPrio(u16),
    AdUserPortKey(u16),
    AdActorSystem([u8; 6]),
    TlbDynamicLb(u8),
    PeerNotifDelay(u32),
    AdLacpActive(u8),
    MissedMax(u8),
    NsIp6Target(Vec<Ipv6Addr>),
    Other(DefaultNla),
}

impl Nla for InfoBond {
    fn value_len(&self) -> usize {
        match self {
            Self::Mode(_)
            | Self::UseCarrier(_)
            | Self::PrimaryReselect(_)
            | Self::FailOverMac(_)
            | Self::XmitHashPolicy(_)
            | Self::NumPeerNotif(_)
            | Self::AllPortsActive(_)
            | Self::AdLacpActive(_)
            | Self::AdLacpRate(_)
            | Self::AdSelect(_)
            | Self::TlbDynamicLb(_)
            | Self::MissedMax(_) => 1,
            Self::AdActorSysPrio(_) | Self::AdUserPortKey(_) => 2,
            Self::ActivePort(_)
            | Self::MiiMon(_)
            | Self::UpDelay(_)
            | Self::DownDelay(_)
            | Self::ArpInterval(_)
            | Self::ArpValidate(_)
            | Self::ArpAllTargets(_)
            | Self::Primary(_)
            | Self::ResendIgmp(_)
            | Self::MinLinks(_)
            | Self::LpInterval(_)
            | Self::PacketsPerPort(_)
            | Self::PeerNotifDelay(_) => 4,
            Self::ArpIpTarget(ref addrs) => {
                BondIpAddrNlaList::from(addrs).as_slice().buffer_len()
            }
            Self::NsIp6Target(ref addrs) => {
                BondIpAddrNlaList::from(addrs).as_slice().buffer_len()
            }
            Self::AdActorSystem(_) => 6,
            Self::AdInfo(infos) => infos.as_slice().buffer_len(),
            Self::Other(v) => v.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Mode(value) => buffer[0] = (*value).into(),
            Self::UseCarrier(value)
            | Self::PrimaryReselect(value)
            | Self::FailOverMac(value)
            | Self::XmitHashPolicy(value)
            | Self::NumPeerNotif(value)
            | Self::AllPortsActive(value)
            | Self::AdLacpActive(value)
            | Self::AdLacpRate(value)
            | Self::AdSelect(value)
            | Self::TlbDynamicLb(value)
            | Self::MissedMax(value) => buffer[0] = *value,
            Self::AdActorSysPrio(value) | Self::AdUserPortKey(value) => {
                NativeEndian::write_u16(buffer, *value)
            }
            Self::ArpValidate(value) => {
                NativeEndian::write_u32(buffer, (*value).into())
            }
            Self::ActivePort(value)
            | Self::MiiMon(value)
            | Self::UpDelay(value)
            | Self::DownDelay(value)
            | Self::ArpInterval(value)
            | Self::ArpAllTargets(value)
            | Self::Primary(value)
            | Self::ResendIgmp(value)
            | Self::MinLinks(value)
            | Self::LpInterval(value)
            | Self::PacketsPerPort(value)
            | Self::PeerNotifDelay(value) => {
                NativeEndian::write_u32(buffer, *value)
            }
            Self::AdActorSystem(bytes) => buffer.copy_from_slice(bytes),
            Self::ArpIpTarget(addrs) => {
                BondIpAddrNlaList::from(addrs).as_slice().emit(buffer)
            }
            Self::NsIp6Target(addrs) => {
                BondIpAddrNlaList::from(addrs).as_slice().emit(buffer)
            }
            Self::AdInfo(infos) => infos.as_slice().emit(buffer),
            Self::Other(v) => v.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Mode(_) => IFLA_BOND_MODE,
            Self::ActivePort(_) => IFLA_BOND_ACTIVE_PORT,
            Self::MiiMon(_) => IFLA_BOND_MIIMON,
            Self::UpDelay(_) => IFLA_BOND_UPDELAY,
            Self::DownDelay(_) => IFLA_BOND_DOWNDELAY,
            Self::UseCarrier(_) => IFLA_BOND_USE_CARRIER,
            Self::ArpInterval(_) => IFLA_BOND_ARP_INTERVAL,
            Self::ArpIpTarget(_) => IFLA_BOND_ARP_IP_TARGET,
            Self::ArpValidate(_) => IFLA_BOND_ARP_VALIDATE,
            Self::ArpAllTargets(_) => IFLA_BOND_ARP_ALL_TARGETS,
            Self::Primary(_) => IFLA_BOND_PRIMARY,
            Self::PrimaryReselect(_) => IFLA_BOND_PRIMARY_RESELECT,
            Self::FailOverMac(_) => IFLA_BOND_FAIL_OVER_MAC,
            Self::XmitHashPolicy(_) => IFLA_BOND_XMIT_HASH_POLICY,
            Self::ResendIgmp(_) => IFLA_BOND_RESEND_IGMP,
            Self::NumPeerNotif(_) => IFLA_BOND_NUM_PEER_NOTIF,
            Self::AllPortsActive(_) => IFLA_BOND_ALL_PORTS_ACTIVE,
            Self::MinLinks(_) => IFLA_BOND_MIN_LINKS,
            Self::LpInterval(_) => IFLA_BOND_LP_INTERVAL,
            Self::PacketsPerPort(_) => IFLA_BOND_PACKETS_PER_PORT,
            Self::AdLacpRate(_) => IFLA_BOND_AD_LACP_RATE,
            Self::AdSelect(_) => IFLA_BOND_AD_SELECT,
            Self::AdInfo(_) => IFLA_BOND_AD_INFO,
            Self::AdActorSysPrio(_) => IFLA_BOND_AD_ACTOR_SYS_PRIO,
            Self::AdUserPortKey(_) => IFLA_BOND_AD_USER_PORT_KEY,
            Self::AdActorSystem(_) => IFLA_BOND_AD_ACTOR_SYSTEM,
            Self::TlbDynamicLb(_) => IFLA_BOND_TLB_DYNAMIC_LB,
            Self::PeerNotifDelay(_) => IFLA_BOND_PEER_NOTIF_DELAY,
            Self::AdLacpActive(_) => IFLA_BOND_AD_LACP_ACTIVE,
            Self::MissedMax(_) => IFLA_BOND_MISSED_MAX,
            Self::NsIp6Target(_) => IFLA_BOND_NS_IP6_TARGET,
            Self::Other(v) => v.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoBond {
    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            IFLA_BOND_MODE => Self::Mode(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_MODE value")?
                    .into(),
            ),
            IFLA_BOND_ACTIVE_PORT => Self::ActivePort(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_ACTIVE_PORT value")?,
            ),
            IFLA_BOND_MIIMON => Self::MiiMon(
                parse_u32(payload).context("invalid IFLA_BOND_MIIMON value")?,
            ),
            IFLA_BOND_UPDELAY => Self::UpDelay(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_UPDELAY value")?,
            ),
            IFLA_BOND_DOWNDELAY => Self::DownDelay(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_DOWNDELAY value")?,
            ),
            IFLA_BOND_USE_CARRIER => Self::UseCarrier(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_USE_CARRIER value")?,
            ),
            IFLA_BOND_ARP_INTERVAL => Self::ArpInterval(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_ARP_INTERVAL value")?,
            ),
            IFLA_BOND_ARP_IP_TARGET => {
                let mut addrs = Vec::<Ipv4Addr>::new();
                for nla in NlasIterator::new(payload) {
                    let nla =
                        &nla.context("invalid IFLA_BOND_ARP_IP_TARGET value")?;
                    if let Ok(IpAddr::V4(addr)) = parse_ip(nla.value()) {
                        addrs.push(addr);
                    }
                }
                Self::ArpIpTarget(addrs)
            }
            IFLA_BOND_ARP_VALIDATE => Self::ArpValidate(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_ARP_VALIDATE value")?
                    .into(),
            ),
            IFLA_BOND_ARP_ALL_TARGETS => Self::ArpAllTargets(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_ARP_ALL_TARGETS value")?,
            ),
            IFLA_BOND_PRIMARY => Self::Primary(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_PRIMARY value")?,
            ),
            IFLA_BOND_PRIMARY_RESELECT => Self::PrimaryReselect(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_PRIMARY_RESELECT value")?,
            ),
            IFLA_BOND_FAIL_OVER_MAC => Self::FailOverMac(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_FAIL_OVER_MAC value")?,
            ),
            IFLA_BOND_XMIT_HASH_POLICY => Self::XmitHashPolicy(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_XMIT_HASH_POLICY value")?,
            ),
            IFLA_BOND_RESEND_IGMP => Self::ResendIgmp(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_RESEND_IGMP value")?,
            ),
            IFLA_BOND_NUM_PEER_NOTIF => Self::NumPeerNotif(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_NUM_PEER_NOTIF value")?,
            ),
            IFLA_BOND_ALL_PORTS_ACTIVE => Self::AllPortsActive(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_ALL_PORTS_ACTIVE value")?,
            ),
            IFLA_BOND_MIN_LINKS => Self::MinLinks(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_MIN_LINKS value")?,
            ),
            IFLA_BOND_LP_INTERVAL => Self::LpInterval(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_LP_INTERVAL value")?,
            ),
            IFLA_BOND_PACKETS_PER_PORT => Self::PacketsPerPort(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_PACKETS_PER_PORT value")?,
            ),
            IFLA_BOND_AD_LACP_RATE => Self::AdLacpRate(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_AD_LACP_RATE value")?,
            ),
            IFLA_BOND_AD_SELECT => Self::AdSelect(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_AD_SELECT value")?,
            ),
            IFLA_BOND_AD_INFO => {
                let mut infos = Vec::new();
                let err = "failed to parse IFLA_BOND_AD_INFO";
                for nla in NlasIterator::new(payload) {
                    let nla = &nla.context(err)?;
                    let info = BondAdInfo::parse(nla).context(err)?;
                    infos.push(info);
                }
                Self::AdInfo(infos)
            }
            IFLA_BOND_AD_ACTOR_SYS_PRIO => Self::AdActorSysPrio(
                parse_u16(payload)
                    .context("invalid IFLA_BOND_AD_ACTOR_SYS_PRIO value")?,
            ),
            IFLA_BOND_AD_USER_PORT_KEY => Self::AdUserPortKey(
                parse_u16(payload)
                    .context("invalid IFLA_BOND_AD_USER_PORT_KEY value")?,
            ),
            IFLA_BOND_AD_ACTOR_SYSTEM => Self::AdActorSystem(
                parse_mac(payload)
                    .context("invalid IFLA_BOND_AD_ACTOR_SYSTEM value")?,
            ),
            IFLA_BOND_TLB_DYNAMIC_LB => Self::TlbDynamicLb(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_TLB_DYNAMIC_LB value")?,
            ),
            IFLA_BOND_PEER_NOTIF_DELAY => Self::PeerNotifDelay(
                parse_u32(payload)
                    .context("invalid IFLA_BOND_PEER_NOTIF_DELAY value")?,
            ),
            IFLA_BOND_AD_LACP_ACTIVE => Self::AdLacpActive(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_AD_LACP_ACTIVE value")?,
            ),
            IFLA_BOND_MISSED_MAX => Self::MissedMax(
                parse_u8(payload)
                    .context("invalid IFLA_BOND_MISSED_MAX value")?,
            ),
            IFLA_BOND_NS_IP6_TARGET => {
                let mut addrs = Vec::<Ipv6Addr>::new();
                for nla in NlasIterator::new(payload) {
                    let nla =
                        &nla.context("invalid IFLA_BOND_NS_IP6_TARGET value")?;
                    if let Ok(IpAddr::V6(addr)) = parse_ip(nla.value()) {
                        addrs.push(addr);
                    }
                }
                Self::NsIp6Target(addrs)
            }
            _ => Self::Other(DefaultNla::parse(buf).context(format!(
                "invalid NLA for {}: {payload:?}",
                buf.kind()
            ))?),
        })
    }
}
