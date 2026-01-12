use cni_plugin::ip_range::IpRange;
use ipnetwork::{IpNetwork, Ipv4Network};
use libipam::config::IPAMConfig;
use libipam::range_set::RangeSet;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};

pub const CNI_VERSION: &str = "1.0.0";
pub const STD_CONF_PATH: &str = "/etc/cni/net.d";

pub const BRIDGE_PLUGIN_NAME: &str = "libbridge";
pub const BRIDGE_CONF: &str = "rkl-standalone-bridge.conf";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliNetworkConfig {
    /// default is 1.0.0
    #[serde(default)]
    pub cni_version: String,
    /// the `type` in JSON
    #[serde(rename = "type")]
    pub plugin: String,
    /// network's name
    #[serde(default)]
    pub name: String,
    /// bridge interface' s name (default cni0）
    #[serde(default)]
    pub bridge: String,
    /// whether this network should be set the container's default gateway
    #[serde(default)]
    pub is_default_gateway: Option<bool>,
    /// whether the bridge should at as a gateway
    #[serde(default)]
    pub is_gateway: Option<bool>,
    /// Maximum Transmission Unit (MTU) to set on the bridge interface
    #[serde(default)]
    pub mtu: Option<u32>,
    /// Enable Mac address spoofing check
    #[serde(default)]
    pub mac_spoof_check: Option<bool>,
    /// IPAM type（like host-local, static, etc.）
    #[serde(default)]
    pub ipam: Option<IPAMConfig>,
    /// enable hairpin mod
    #[serde(default)]
    pub hairpin_mode: Option<bool>,
    /// VLAN ID
    #[serde(default)]
    pub vlan: Option<u16>,
    /// VLAN Trunk
    #[serde(default)]
    pub vlan_trunk: Option<Vec<u16>>,
}

impl CliNetworkConfig {
    pub fn from_name_bridge(network_name: &str, bridge: &str) -> Self {
        Self {
            bridge: bridge.to_string(),
            name: network_name.to_string(),
            ..Default::default()
        }
    }

    /// Due to compose will need a unique subnet
    /// this function need two extra parameter
    /// - subnet_addr
    /// - gateway_addr
    ///
    /// by default the subnet prefix is 16
    pub fn from_subnet_gateway(
        network_name: &str,
        bridge: &str,
        subnet_addr: Ipv4Addr,
        getway_addr: Ipv4Addr,
    ) -> Self {
        let ip_range = IpRange {
            subnet: IpNetwork::V4(Ipv4Network::new(subnet_addr, 16).unwrap()),
            range_start: None,
            range_end: None,
            gateway: Some(IpAddr::V4(getway_addr)),
        };

        let set: RangeSet = vec![ip_range];

        Self {
            bridge: bridge.to_string(),
            name: network_name.to_string(),
            ipam: Some(IPAMConfig {
                type_field: "libipam".to_string(),
                name: None,
                routes: None,
                resolv_conf: None,
                data_dir: None,
                ranges: vec![set],
                ip_args: vec![],
            }),
            ..Default::default()
        }
    }
}

impl Default for CliNetworkConfig {
    fn default() -> Self {
        // Default subnet-addr for rkl container management
        // 172.17.0.0/16
        let subnet_addr = Ipv4Addr::new(172, 17, 0, 0);
        let getway_addr = Ipv4Addr::new(172, 17, 0, 1);

        let ip_range = IpRange {
            subnet: ipnetwork::IpNetwork::V4(Ipv4Network::new(subnet_addr, 16).unwrap()),
            range_start: None,
            range_end: None,
            gateway: Some(IpAddr::V4(getway_addr)),
        };

        let set: RangeSet = vec![ip_range];

        Self {
            cni_version: String::from(CNI_VERSION),
            plugin: String::from(BRIDGE_PLUGIN_NAME),
            name: Default::default(),
            bridge: Default::default(),
            is_default_gateway: Default::default(),
            is_gateway: Some(true),
            mtu: Some(1500),
            mac_spoof_check: Default::default(),
            hairpin_mode: Default::default(),
            vlan: Default::default(),
            vlan_trunk: Default::default(),
            ipam: Some(IPAMConfig {
                type_field: "libipam".to_string(),
                name: None,
                routes: None,
                resolv_conf: None,
                data_dir: None,
                ranges: vec![set],
                ip_args: vec![],
            }),
        }
    }
}
