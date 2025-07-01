use std::collections::HashMap;

use cni_plugin::config::NetworkConfig;
use ipnetwork::{Ipv4Network, Ipv6Network};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bridge network configuration structure, extending `NetworkConfig`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlannelNetConf {
    #[serde(flatten)]
    pub net_conf: NetworkConfig, // Embed `NetworkConfig`

    // Bridge-related configuration
    #[serde(
        rename = "subnetFile",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub subnet_file: Option<String>,
    #[serde(rename = "dataDir", default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,
    #[serde(rename = "delegate", default, skip_serializing_if = "Option::is_none")]
    pub delegate: Option<HashMap<String, Value>>,
}

#[derive(Debug)]
pub struct SubnetEnv {
    pub networks: Vec<Ipv4Network>,
    pub subnet: Option<Ipv4Network>,
    pub ip6_networks: Vec<Ipv6Network>,
    pub ip6_subnet: Option<Ipv6Network>,
    pub mtu: Option<u32>,
    pub ipmasq: Option<bool>,
}
