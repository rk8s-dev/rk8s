// Copyright (c) 2024 https://github.com/divinerapier/cni-rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Clone)]
pub struct Config {
    pub plugin_dirs: Vec<String>,
    pub plugin_conf_dir: String,
    pub plugin_max_conf_num: i64,
    pub prefix: String,
}
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct NetworkConfig {
    pub network: NetConf,
    pub bytes: Vec<u8>,
}

// impl <T> NetworkConfig<T> {
//     pub fn new(bytes: &[u8]) ->
// }
#[derive(Default, Clone, Serialize, Deserialize, Debug)]
pub struct NetConf {
    #[serde(default, alias = "cniVersion")]
    pub cni_version: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, alias = "type")]
    pub _type: String,
    #[serde(default)]
    pub capabilities: HashMap<String, bool>,
}

#[derive(Serialize, Deserialize)]
pub struct IPAM {
    #[serde(rename = "type")]
    pub _type: String,
}

pub struct NetConfList {
    pub cni_version: String,
    pub name: String,
    pub disable_check: bool,
    pub plugins: Vec<NetworkConfig>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct DNS {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nameservers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Route {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "dst")]
    pub dst: Option<ipnetwork::IpNetwork>,
    #[serde(rename = "gw")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gw: Option<std::net::IpAddr>,
}
