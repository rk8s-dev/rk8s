use std::collections::HashMap;
use std::fs;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use crate::commands::compose::spec::NetworkDriver::Bridge;
use crate::commands::compose::spec::NetworkDriver::Host;
use crate::commands::compose::spec::NetworkDriver::Overlay;

use cni_plugin::ip_range::IpRange;
use ipnetwork::IpNetwork;
use ipnetwork::Ipv4Network;
use libipam::config::IPAMConfig;
use libipam::range_set::RangeSet;

use crate::commands::compose::spec::ComposeSpec;
use crate::commands::compose::spec::NetworkSpec;
use crate::commands::compose::spec::ServiceSpec;
use anyhow::Ok;
use anyhow::Result;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};

pub const CNI_VERSION: &str = "1.0.0";
pub const STD_CONF_PATH: &str = "/etc/cni/net.d";

pub const BRIDGE_PLUGIN_NAME: &str = "libbridge";
pub const BRIDGE_CONF: &str = "rkl-standalone-bridge.conf";

#[derive(Debug, Serialize, Deserialize)]
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
        let subnet_addr = Ipv4Addr::new(10, 10, 0, 0);
        let getway_addr = Ipv4Addr::new(10, 10, 2, 1);

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

pub struct NetworkManager {
    map: HashMap<String, NetworkSpec>,
    /// key: network_name; value: bridge interface
    network_interface: HashMap<String, String>,
    /// key: service_name value: networks
    service_mapping: HashMap<String, Vec<String>>,
    /// key: network_name value: (srv_name, service_spec)
    network_service: HashMap<String, Vec<(String, ServiceSpec)>>,
    /// if there is no network definition then just create a default network
    is_default: bool,
    project_name: String,
}

impl NetworkManager {
    pub fn new(project_name: String) -> Self {
        Self {
            map: HashMap::new(),
            service_mapping: HashMap::new(),
            is_default: false,
            network_service: HashMap::new(),
            project_name,
            network_interface: HashMap::new(),
        }
    }

    pub fn network_service_mapping(&self) -> HashMap<String, Vec<(String, ServiceSpec)>> {
        self.network_service.clone()
    }

    pub fn setup_network_conf(&self, network_name: &String) -> Result<()> {
        // generate the config file
        let interface = self.network_interface.get(network_name).ok_or_else(|| {
            anyhow!(
                "Failed to find bridge interface for network {}",
                network_name
            )
        })?;

        let subnet_addr = Ipv4Addr::new(10, 20, 0, 0);
        let gateway_addr = Ipv4Addr::new(10, 20, 0, 1);

        let conf = CliNetworkConfig::from_subnet_gateway(
            network_name,
            interface,
            subnet_addr,
            gateway_addr,
        );

        let conf_value = serde_json::to_value(conf).expect("Failed to parse network config");

        let mut conf_path = PathBuf::from(STD_CONF_PATH);
        conf_path.push(BRIDGE_CONF);
        if let Some(parent) = conf_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
        }

        // write it to
        fs::write(conf_path, serde_json::to_string_pretty(&conf_value)?)?;

        Ok(())
    }

    pub fn handle(&mut self, spec: &ComposeSpec) -> Result<()> {
        // read the networks
        if let Some(networks_spec) = &spec.networks {
            self.map = networks_spec.clone()
        } else {
            // there is no definition of networks
            self.is_default = true
        }

        self.validate(spec)?;

        // allocate the bridge interface
        self.allocate_interface()
    }

    /// validate the correctness and initialize  the service_mapping
    fn validate(&mut self, spec: &ComposeSpec) -> Result<()> {
        for (srv, srv_spec) in &spec.services {
            // if the srv does not have the network definition then add to the default network
            let network_name = format!("{}_default", self.project_name);
            if srv_spec.networks.is_empty() {
                self.network_service
                    .entry(network_name.clone())
                    .or_default()
                    .push((srv.clone(), srv_spec.clone()));

                // add to map
                self.map.insert(
                    network_name,
                    NetworkSpec {
                        external: Option::None,
                        driver: Some(Bridge),
                    },
                );
            }
            for network_name in &srv_spec.networks {
                if !self.map.contains_key(network_name) {
                    return Err(anyhow!(
                        "bad network's definition network {} is not defined",
                        network_name
                    ));
                }
                self.service_mapping
                    .entry(srv.clone())
                    .or_default()
                    .push(network_name.clone());

                self.network_service
                    .entry(network_name.clone())
                    .or_default()
                    .push((srv.clone(), srv_spec.clone()));
            }
        }
        // all the services don't have the network definition then create a default network
        if self.is_default {
            let network_name = format!("{}_default", self.project_name);

            let services: Vec<(String, ServiceSpec)> = spec
                .services
                .iter()
                .map(|(name, spec)| (name.clone(), spec.clone()))
                .collect();

            self.network_service.insert(network_name.clone(), services);
            self.map.insert(
                network_name,
                NetworkSpec {
                    external: Option::None,
                    driver: Some(Bridge),
                },
            );
        }
        Ok(())
    }

    fn allocate_interface(&mut self) -> Result<()> {
        for (i, (k, v)) in self.map.iter().enumerate() {
            if let Some(driver) = &v.driver {
                match driver {
                    // add the bridge default is rCompose0
                    Bridge => self
                        .network_interface
                        .insert(k.to_string(), format!("rCompose{}", i + 1).to_string()),
                    Overlay => todo!(),
                    Host => todo!(),
                };
            }
        }
        Ok(())
    }
}
