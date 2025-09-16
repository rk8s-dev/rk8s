// SPDX-License-Identifier: MIT

use std::net::{Ipv4Addr, Ipv6Addr};

use crate::{
    link::LinkMessageBuilder,
    packet_route::link::{
        BondArpValidate, BondMode, InfoBond, InfoData, InfoKind,
    },
};

///
/// Represent bond interface.
/// Example code on creating a bond interface
/// ```no_run
/// use rtnetlink::{new_connection, LinkBond, packet_route::link::BondMode};
///
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().map_err(|e|
///         format!("{e}"))?;
///     tokio::spawn(connection);
///
///     let message = LinkBond::new("bond0")
///        .mode(BondMode::ActiveBackup)
///        .miimon(100)
///        .updelay(100)
///        .downdelay(100)
///        .min_links(2)
///        .up()
///        .build();
///
///     handle
///         .link()
///         .add(message)
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkBond> for more detail.
#[derive(Debug)]
pub struct LinkBond;

impl LinkBond {
    /// Equal to `LinkMessageBuilder::<LinkBond>::new()`
    pub fn new(name: &str) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkBond>::new(name)
    }
}

impl LinkMessageBuilder<LinkBond> {
    /// Create [LinkMessageBuilder] for bond
    pub fn new(name: &str) -> Self {
        LinkMessageBuilder::<LinkBond>::new_with_info_kind(InfoKind::Bond)
            .name(name.to_string())
    }

    pub fn append_info_data(self, info: InfoBond) -> Self {
        let mut ret = self;
        if let InfoData::Bond(infos) = ret
            .info_data
            .get_or_insert_with(|| InfoData::Bond(Vec::new()))
        {
            infos.push(info);
        }
        ret
    }

    /// Adds the `mode` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond mode MODE`.
    pub fn mode(self, mode: BondMode) -> Self {
        self.append_info_data(InfoBond::Mode(mode))
    }

    /// Adds the `active_port` attribute to the bond, where `active_port`
    /// is the ifindex of an interface attached to the bond.
    /// This is equivalent to `ip link add name NAME type bond active_slave
    /// ACTIVE_PORT_NAME`.
    pub fn active_port(self, active_port: u32) -> Self {
        self.append_info_data(InfoBond::ActivePort(active_port))
    }

    /// Adds the `miimon` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond miimon MIIMON`.
    pub fn miimon(self, miimon: u32) -> Self {
        self.append_info_data(InfoBond::MiiMon(miimon))
    }

    /// Adds the `updelay` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond updelay UPDELAY`.
    pub fn updelay(self, updelay: u32) -> Self {
        self.append_info_data(InfoBond::UpDelay(updelay))
    }

    /// Adds the `downdelay` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond downdelay
    /// DOWNDELAY`.
    pub fn downdelay(self, downdelay: u32) -> Self {
        self.append_info_data(InfoBond::DownDelay(downdelay))
    }

    /// Adds the `use_carrier` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond use_carrier
    /// USE_CARRIER`.
    pub fn use_carrier(self, use_carrier: u8) -> Self {
        self.append_info_data(InfoBond::UseCarrier(use_carrier))
    }

    /// Adds the `arp_interval` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond arp_interval
    /// ARP_INTERVAL`.
    pub fn arp_interval(self, arp_interval: u32) -> Self {
        self.append_info_data(InfoBond::ArpInterval(arp_interval))
    }

    /// Adds the `arp_validate` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond arp_validate
    /// ARP_VALIDATE`.
    pub fn arp_validate(self, arp_validate: BondArpValidate) -> Self {
        self.append_info_data(InfoBond::ArpValidate(arp_validate))
    }

    /// Adds the `arp_all_targets` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond arp_all_targets
    /// ARP_ALL_TARGETS`
    pub fn arp_all_targets(self, arp_all_targets: u32) -> Self {
        self.append_info_data(InfoBond::ArpAllTargets(arp_all_targets))
    }

    /// Adds the `primary` attribute to the bond, where `primary` is the ifindex
    /// of an interface.
    /// This is equivalent to `ip link add name NAME type bond primary
    /// PRIMARY_NAME`
    pub fn primary(self, primary: u32) -> Self {
        self.append_info_data(InfoBond::Primary(primary))
    }

    /// Adds the `primary_reselect` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond primary_reselect
    /// PRIMARY_RESELECT`.
    pub fn primary_reselect(self, primary_reselect: u8) -> Self {
        self.append_info_data(InfoBond::PrimaryReselect(primary_reselect))
    }

    /// Adds the `fail_over_mac` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond fail_over_mac
    /// FAIL_OVER_MAC`.
    pub fn fail_over_mac(self, fail_over_mac: u8) -> Self {
        self.append_info_data(InfoBond::FailOverMac(fail_over_mac))
    }

    /// Adds the `xmit_hash_policy` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond xmit_hash_policy
    /// XMIT_HASH_POLICY`.
    pub fn xmit_hash_policy(self, xmit_hash_policy: u8) -> Self {
        self.append_info_data(InfoBond::XmitHashPolicy(xmit_hash_policy))
    }

    /// Adds the `resend_igmp` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond resend_igmp
    /// RESEND_IGMP`.
    pub fn resend_igmp(self, resend_igmp: u32) -> Self {
        self.append_info_data(InfoBond::ResendIgmp(resend_igmp))
    }

    /// Adds the `num_peer_notif` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond num_peer_notif
    /// NUM_PEER_NOTIF`.
    pub fn num_peer_notif(self, num_peer_notif: u8) -> Self {
        self.append_info_data(InfoBond::NumPeerNotif(num_peer_notif))
    }

    /// Adds the `all_ports_active` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond all_slaves_active
    /// ALL_PORTS_ACTIVE`.
    pub fn all_ports_active(self, all_ports_active: u8) -> Self {
        self.append_info_data(InfoBond::AllPortsActive(all_ports_active))
    }

    /// Adds the `min_links` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond min_links
    /// MIN_LINKS`.
    pub fn min_links(self, min_links: u32) -> Self {
        self.append_info_data(InfoBond::MinLinks(min_links))
    }

    /// Adds the `lp_interval` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond lp_interval
    /// LP_INTERVAL`.
    pub fn lp_interval(self, lp_interval: u32) -> Self {
        self.append_info_data(InfoBond::LpInterval(lp_interval))
    }

    /// Adds the `packets_per_port` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond packets_per_slave
    /// PACKETS_PER_PORT`.
    pub fn packets_per_port(self, packets_per_port: u32) -> Self {
        self.append_info_data(InfoBond::PacketsPerPort(packets_per_port))
    }

    /// Adds the `ad_lacp_rate` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond ad_lacp_rate
    /// AD_LACP_RATE`.
    pub fn ad_lacp_rate(self, ad_lacp_rate: u8) -> Self {
        self.append_info_data(InfoBond::AdLacpRate(ad_lacp_rate))
    }

    /// Adds the `ad_select` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond ad_select
    /// AD_SELECT`.
    pub fn ad_select(self, ad_select: u8) -> Self {
        self.append_info_data(InfoBond::AdSelect(ad_select))
    }

    /// Adds the `ad_actor_sys_prio` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond ad_actor_sys_prio
    /// AD_ACTOR_SYS_PRIO`.
    pub fn ad_actor_sys_prio(self, ad_actor_sys_prio: u16) -> Self {
        self.append_info_data(InfoBond::AdActorSysPrio(ad_actor_sys_prio))
    }

    /// Adds the `ad_user_port_key` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond ad_user_port_key
    /// AD_USER_PORT_KEY`.
    pub fn ad_user_port_key(self, ad_user_port_key: u16) -> Self {
        self.append_info_data(InfoBond::AdUserPortKey(ad_user_port_key))
    }

    /// Adds the `ad_actor_system` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond ad_actor_system
    /// AD_ACTOR_SYSTEM`.
    pub fn ad_actor_system(self, ad_actor_system: [u8; 6]) -> Self {
        self.append_info_data(InfoBond::AdActorSystem(ad_actor_system))
    }

    /// Adds the `tlb_dynamic_lb` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond tlb_dynamic_lb
    /// TLB_DYNAMIC_LB`.
    pub fn tlb_dynamic_lb(self, tlb_dynamic_lb: u8) -> Self {
        self.append_info_data(InfoBond::TlbDynamicLb(tlb_dynamic_lb))
    }

    /// Adds the `peer_notif_delay` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond peer_notif_delay
    /// PEER_NOTIF_DELAY`.
    pub fn peer_notif_delay(self, peer_notif_delay: u32) -> Self {
        self.append_info_data(InfoBond::PeerNotifDelay(peer_notif_delay))
    }

    /// Adds the `ad_lacp_active` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond ad_lacp_active
    /// AD_LACP_ACTIVE`.
    pub fn ad_lacp_active(self, ad_lacp_active: u8) -> Self {
        self.append_info_data(InfoBond::AdLacpActive(ad_lacp_active))
    }

    /// Adds the `missed_max` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond missed_max
    /// MISSED_MAX`.
    pub fn missed_max(self, missed_max: u8) -> Self {
        self.append_info_data(InfoBond::MissedMax(missed_max))
    }

    /// Adds the `arp_ip_target` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond arp_ip_target
    /// LIST`.
    pub fn arp_ip_target(self, arp_ip_target: Vec<Ipv4Addr>) -> Self {
        self.append_info_data(InfoBond::ArpIpTarget(arp_ip_target))
    }

    /// Adds the `ns_ip6_target` attribute to the bond
    /// This is equivalent to `ip link add name NAME type bond ns_ip6_target
    /// LIST`.
    pub fn ns_ip6_target(self, ns_ip6_target: Vec<Ipv6Addr>) -> Self {
        self.append_info_data(InfoBond::NsIp6Target(ns_ip6_target))
    }
}
