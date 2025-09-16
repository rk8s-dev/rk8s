// SPDX-License-Identifier: MIT

use crate::{
    packet_route::link::{InfoData, InfoKind, InfoVxlan},
    LinkMessageBuilder,
};

/// Represent VxLAN interface.
/// Example code on creating a VxLAN interface
/// ```no_run
/// use rtnetlink::{new_connection, LinkVxlan};
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     handle
///         .link()
///         .add(LinkVxlan::new("vxlan100", 100)
///         .dev(10)
///         .port(4789)
///         .up()
///         .build())
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
///
/// Please check LinkMessageBuilder::<LinkVlan> for more detail.
#[derive(Debug)]
pub struct LinkVxlan;

impl LinkVxlan {
    /// Wrapper of `LinkMessageBuilder::<LinkVxlan>::new().id()`
    pub fn new(name: &str, vni: u32) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkVxlan>::new(name).id(vni)
    }
}

impl LinkMessageBuilder<LinkVxlan> {
    /// Create [LinkMessageBuilder] for VLAN
    pub fn new(name: &str) -> Self {
        LinkMessageBuilder::<LinkVxlan>::new_with_info_kind(InfoKind::Vxlan)
            .name(name.to_string())
    }

    pub fn append_info_data(self, info: InfoVxlan) -> Self {
        let mut ret = self;
        if let InfoData::Vxlan(infos) = ret
            .info_data
            .get_or_insert_with(|| InfoData::Vxlan(Vec::new()))
        {
            infos.push(info);
        }
        ret
    }

    /// VNI
    pub fn id(self, id: u32) -> Self {
        self.append_info_data(InfoVxlan::Id(id))
    }

    /// This is equivalent to `devPHYS_DEV` for ip link vxlan.
    /// Adds the `dev` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI dev
    /// LINK`,  dev LINK - specifies the physical device to use
    ///  for tunnel endpoint communication.
    /// But instead of specifing a link name (`LINK`), we specify a link index.
    /// Please be aware the `LinkMessageBuilder::link()` will not works for
    /// VxLAN.
    pub fn dev(self, index: u32) -> Self {
        self.append_info_data(InfoVxlan::Link(index))
    }

    /// Adds the `dstport` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI dstport
    /// PORT`. dstport PORT - specifies the UDP destination port to
    /// communicate to the remote VXLAN tunnel endpoint.
    pub fn port(self, port: u16) -> Self {
        self.append_info_data(InfoVxlan::Port(port))
    }

    /// Adds the `group` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI group
    /// IPADDR`, group IPADDR - specifies the multicast IP address to join.
    /// This function takes an IPv4 address
    /// WARNING: only one between `remote` and `group` can be present.
    pub fn group(self, addr: std::net::Ipv4Addr) -> Self {
        self.append_info_data(InfoVxlan::Group(addr))
    }

    /// Adds the `group` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI group
    /// IPADDR`, group IPADDR - specifies the multicast IP address to join.
    /// This function takes an IPv6 address
    /// WARNING: only one between `remote` and `group` can be present.
    pub fn group6(self, addr: std::net::Ipv6Addr) -> Self {
        self.append_info_data(InfoVxlan::Group6(addr))
    }

    /// Adds the `remote` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI remote
    /// IPADDR`, remote IPADDR - specifies the unicast destination IP
    /// address to use in outgoing packets when the
    /// destination link layer address is not known in the
    /// VXLAN device forwarding database.
    /// This function takes an IPv4 address.
    /// WARNING: only one between `remote` and `group` can be present.
    pub fn remote(self, addr: std::net::Ipv4Addr) -> Self {
        self.group(addr)
    }

    /// Adds the `remote` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI remote
    /// IPADDR`, remote IPADDR - specifies the unicast destination IP
    /// address to use in outgoing packets when the
    /// destination link layer address is not known in the
    /// VXLAN device forwarding database.
    /// This function takes an IPv6 address.
    /// WARNING: only one between `remote` and `group` can be present.
    pub fn remote6(self, addr: std::net::Ipv6Addr) -> Self {
        self.group6(addr)
    }

    /// Adds the `local` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI local
    /// IPADDR`, local IPADDR - specifies the source IP address to use in
    /// outgoing packets. This function takes an IPv4 address.
    pub fn local(self, addr: std::net::Ipv4Addr) -> Self {
        self.append_info_data(InfoVxlan::Local(addr))
    }

    /// Adds the `local` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI local
    /// IPADDR`, local IPADDR - specifies the source IP address to use in
    /// outgoing packets. This function takes an IPv6 address.
    pub fn local6(self, addr: std::net::Ipv6Addr) -> Self {
        self.append_info_data(InfoVxlan::Local6(addr))
    }

    /// Adds the `tos` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI tos TOS`.
    /// tos TOS - specifies the TOS value to use in outgoing packets.
    pub fn tos(self, tos: u8) -> Self {
        self.append_info_data(InfoVxlan::Tos(tos))
    }

    /// Adds the `ttl` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI ttl TTL`.
    /// ttl TTL - specifies the TTL value to use in outgoing packets.
    pub fn ttl(self, ttl: u8) -> Self {
        self.append_info_data(InfoVxlan::Ttl(ttl))
    }

    /// Adds the `flowlabel` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI flowlabel
    /// LABEL`. flowlabel LABEL - specifies the flow label to use in
    /// outgoing packets.
    pub fn label(self, label: u32) -> Self {
        self.append_info_data(InfoVxlan::Label(label))
    }

    /// Adds the `learning` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI
    /// \[no\]learning`. \[no\]learning - specifies if unknown source link layer
    /// addresses and IP addresses are entered into the VXLAN
    /// device forwarding database.
    pub fn learning(self, learning: bool) -> Self {
        self.append_info_data(InfoVxlan::Learning(learning))
    }

    /// Adds the `ageing` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI ageing
    /// SECONDS`. ageing SECONDS - specifies the lifetime in seconds of
    /// FDB entries learnt by the kernel.
    pub fn ageing(self, seconds: u32) -> Self {
        self.append_info_data(InfoVxlan::Ageing(seconds))
    }

    /// Adds the `maxaddress` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI
    /// maxaddress LIMIT`. maxaddress LIMIT - specifies the maximum number
    /// of FDB entries.
    pub fn limit(self, limit: u32) -> Self {
        self.append_info_data(InfoVxlan::Limit(limit))
    }

    /// Adds the `srcport` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI srcport
    /// MIN MAX`. srcport MIN MAX - specifies the range of port numbers
    /// to use as UDP source ports to communicate to the
    /// remote VXLAN tunnel endpoint.
    pub fn port_range(self, min: u16, max: u16) -> Self {
        self.append_info_data(InfoVxlan::PortRange((min, max)))
    }

    /// Adds the `proxy` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI
    /// [no]proxy`. \[no\]proxy - specifies ARP proxy is turned on.
    pub fn proxy(self, proxy: bool) -> Self {
        self.append_info_data(InfoVxlan::Proxy(proxy))
    }

    /// Adds the `rsc` attribute to the VXLAN This is equivalent to
    /// `ip link add name NAME type vxlan id VNI [no]rsc`.
    /// \[no\]rsc - specifies if route short circuit is turned on.
    pub fn rsc(self, rsc: bool) -> Self {
        self.append_info_data(InfoVxlan::Rsc(rsc))
    }

    // Adds the `l2miss` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI
    /// [no]l2miss`. \[no\]l2miss - specifies if netlink LLADDR miss
    /// notifications are generated.
    pub fn l2miss(self, l2miss: bool) -> Self {
        self.append_info_data(InfoVxlan::L2Miss(l2miss))
    }

    // Adds the `l3miss` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI
    /// [no]l3miss`. \[no\]l3miss - specifies if netlink IP ADDR
    /// miss notifications are generated.
    pub fn l3miss(self, l3miss: bool) -> Self {
        self.append_info_data(InfoVxlan::L3Miss(l3miss))
    }

    pub fn collect_metadata(self, collect_metadata: bool) -> Self {
        self.append_info_data(InfoVxlan::CollectMetadata(collect_metadata))
    }

    // Adds the `udp_csum` attribute to the VXLAN
    /// This is equivalent to `ip link add name NAME type vxlan id VNI
    /// [no]udp_csum`. \[no\]udpcsum - specifies if UDP checksum is
    /// calculated for transmitted packets over IPv4.
    pub fn udp_csum(self, udp_csum: bool) -> Self {
        self.append_info_data(InfoVxlan::UDPCsum(udp_csum))
    }
}
