use cni_plugin::config::NetworkConfig;
use ipnetwork::IpNetwork;
use netlink_packet_route::AddressFamily;
use rtnetlink::LinkMessageBuilder;
use serde::{Deserialize, Serialize};

/// Bridge network configuration structure, extending `NetworkConfig`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeNetConf {
    #[serde(flatten)]
    pub net_conf: NetworkConfig, // Embed `NetworkConfig`

    // Bridge-related configuration
    #[serde(rename = "bridge", default, skip_serializing_if = "Option::is_none")]
    pub br_name: Option<String>,
    #[serde(rename = "isGateway", default, skip_serializing_if = "Option::is_none")]
    pub is_gw: Option<bool>,
    #[serde(
        rename = "isDefaultGateway",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub is_default_gw: Option<bool>,
    #[serde(
        rename = "forceAddress",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub force_address: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
    #[serde(
        rename = "hairpinMode",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub hairpin_mode: Option<bool>,
    #[serde(
        rename = "promiscMode",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub promisc_mode: Option<bool>,

    // VLAN-related configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vlan: Option<i32>,
    #[serde(rename = "vlanTrunk", default, skip_serializing_if = "Option::is_none")]
    pub vlan_trunk: Option<Vec<VlanTrunk>>,
    #[serde(
        rename = "preserveDefaultVlan",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub preserve_default_vlan: Option<bool>,

    // MAC-related configuration
    #[serde(
        rename = "macspoofchk",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub mac_spoof_chk: Option<bool>,
    #[serde(rename = "enabledad", default, skip_serializing_if = "Option::is_none")]
    pub enable_dad: Option<bool>,

    // Additional fields
    #[serde(
        rename = "disableContainerInterface",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub disable_container_interface: Option<bool>,
    #[serde(
        rename = "portIsolation",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub port_isolation: Option<bool>,

    // Arguments for bridge configuration
    #[serde(rename = "args", default, skip_serializing_if = "Option::is_none")]
    pub args: Option<BridgeArgs>,

    // Other arguments
    pub mac: Option<String>,
    pub vlans: Option<Vec<i32>>,
}

/// VLAN trunk configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlanTrunk {
    #[serde(rename = "minID", skip_serializing_if = "Option::is_none")]
    pub min_id: Option<i32>,
    #[serde(rename = "maxID", skip_serializing_if = "Option::is_none")]
    pub max_id: Option<i32>,
    #[serde(rename = "id", skip_serializing_if = "Option::is_none")]
    pub id: Option<i32>,
}

/// Bridge-specific arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeArgs {
    #[serde(rename = "mac", default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
}

/// Bridge structure representing a virtual network bridge.
#[derive(Debug, Clone)]
pub struct Bridge {
    pub name: String,         // Bridge name
    pub mtu: u32,             // Bridge MTU size
    pub vlan_filtering: bool, // VLAN filtering enabled/disabled
}

impl Bridge {
    /// Creates a new Bridge instance with default settings.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            mtu: 1500,             // Default MTU
            vlan_filtering: false, // Default VLAN filtering setting
        }
    }

    /// Sets the MTU for the bridge.
    pub fn mtu(mut self, mtu: u32) -> Self {
        self.mtu = mtu;
        self
    }

    /// Enables or disables VLAN filtering on the bridge.
    pub fn set_vlan_filtering(mut self, vlan_filtering: bool) -> Self {
        self.vlan_filtering = vlan_filtering;
        self
    }

    /// Converts the Bridge instance into a LinkMessageBuilder.
    pub fn into_builder(self) -> LinkMessageBuilder<rtnetlink::LinkBridge> {
        LinkMessageBuilder::<rtnetlink::LinkBridge>::new(&self.name).mtu(self.mtu)
    }
}

#[derive(Debug, Default)]
pub struct GatewayInfo {
    pub gws: Vec<IpNetwork>,
    pub family: AddressFamily,
    pub default_route_found: bool,
}