use ipnet::IpNet;
use libruntime::cri::cri_api::Mount;
use netavark::commands::setup::Setup;
use netavark::commands::teardown::Teardown;
use netavark::network::types::Network;
use netavark::network::types::NetworkOptions;
use netavark::network::types::PerNetworkOptions;
use netavark::network::types::Subnet;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::path::Path;
use std::path::PathBuf;

use crate::commands::compose::spec::NetworkDriver::Bridge;
use crate::commands::compose::spec::NetworkDriver::Host;
use crate::commands::compose::spec::NetworkDriver::Overlay;
use crate::commands::compose::spec::NetworkSpec;

use cni_plugin::ip_range::IpRange;
use ipnetwork::IpNetwork;
use ipnetwork::Ipv4Network;
use libipam::config::IPAMConfig;
use libipam::range_set::RangeSet;

use crate::commands::compose::spec::ComposeSpec;
use crate::commands::compose::spec::ServiceSpec;
use crate::commands::container::ContainerRunner;
use anyhow::Result;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};

pub const CNI_VERSION: &str = "1.0.0";
pub const STD_CONF_PATH: &str = "/etc/cni/net.d";

pub const BRIDGE_PLUGIN_NAME: &str = "libbridge";
pub const BRIDGE_CONF: &str = "rkl-standalone-bridge.conf";
/// Subnet size for each compose network (256 IPs per network)
/// Follows Podman's default subnet allocation strategy
const DEFAULT_SUBNET_POOL_PREFIX: u8 = 24;

/// Single pool: 172.17.0.0/16, each compose network allocates a /24 (e.g. 172.17.0.0/24, 172.17.1.0/24, ...).
fn default_subnet_pools() -> Vec<(Ipv4Network, u8)> {
    vec![(
        Ipv4Network::new(Ipv4Addr::new(172, 17, 0, 0), 16).unwrap(),
        DEFAULT_SUBNET_POOL_PREFIX,
    )]
}

fn network_intersects(a: &Ipv4Network, b: &Ipv4Network) -> bool {
    b.contains(a.network()) || a.contains(b.network())
}

fn next_subnet(subnet: &Ipv4Network) -> Option<Ipv4Network> {
    let prefix = subnet.prefix();
    if prefix == 0 {
        return None;
    }
    let base = subnet.network();
    let base_u32 = u32::from(base);
    let inc = 1u32.checked_shl((32 - prefix) as u32)?;
    let next_u32 = base_u32.checked_add(inc)?;
    let next_addr = Ipv4Addr::from(next_u32.to_be_bytes());
    Ipv4Network::new(next_addr, prefix).ok()
}

/// Get first free IPv4 subnet from pools that does not intersect with used. Returns (subnet, gateway).
/// Gateway is first host in subnet (Podman convention). Same algorithm as GetFreeIPv4NetworkSubnet in podman
fn get_free_ipv4_subnet(
    used_networks: &[Ipv4Network],
    pools: &[(Ipv4Network, u8)],
) -> Option<(Ipv4Network, Ipv4Addr)> {
    for (base_pool, size) in pools {
        let net_ip = base_pool.network();
        let mut network = Ipv4Network::new(net_ip, *size).ok()?;
        while base_pool.contains(network.network()) {
            let intersects = used_networks
                .iter()
                .any(|used| network_intersects(&network, used));
            if !intersects {
                let gateway = first_host_in_subnet(&network);
                return Some((network, gateway));
            }
            network = next_subnet(&network)?;
        }
    }
    None
}

fn first_host_in_subnet(subnet: &Ipv4Network) -> Ipv4Addr {
    let base = subnet.network();
    let n = u32::from(base);
    let first_host = n.checked_add(1).unwrap_or(n);
    Ipv4Addr::from(first_host.to_be_bytes())
}

fn default_netavark_config_dir() -> OsString {
    if let Some(v) = std::env::var_os("NETAVARK_CONFIG") {
        return v;
    }

    OsString::from("/run/containers/networks")
}

fn default_aardvark_bin() -> Result<OsString> {
    if let Some(v) = std::env::var_os("AARDVARK_DNS_BIN") {
        return Ok(v);
    }
    if let Some(v) = std::env::var_os("AARDVARK_BIN") {
        return Ok(v);
    }

    let candidates = ["/usr/libexec/podman/aardvark-dns", "/usr/bin/aardvark-dns"];
    for c in candidates {
        if Path::new(c).exists() {
            return Ok(OsString::from(c));
        }
    }

    Err(anyhow!("aardvark-dns not found"))
}

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
    /// Compose networks use Podman-style /24 from subnet pool. prefix is typically 24.
    pub fn from_subnet_gateway(
        network_name: &str,
        bridge: &str,
        subnet_addr: Ipv4Addr,
        gateway_addr: Ipv4Addr,
        prefix: u8,
    ) -> Self {
        let ip_range = IpRange {
            subnet: IpNetwork::V4(Ipv4Network::new(subnet_addr, prefix).unwrap()),
            range_start: None,
            range_end: None,
            gateway: Some(IpAddr::V4(gateway_addr)),
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

#[derive(Debug)]
struct NetworkSubnet {
    subnet: Ipv4Network,
    gateway: Ipv4Addr,
}

/// Next IPv4 to assign in a subnet
fn next_container_ip_in_subnet(
    subnet: &Ipv4Network,
    gateway: Ipv4Addr,
    last_used: Option<Ipv4Addr>,
) -> Option<Ipv4Addr> {
    let gw_u32 = u32::from(gateway);
    let start = gw_u32 + 1;
    let next: u32 = last_used
        .and_then(|u| u32::from(u).checked_add(1))
        .unwrap_or(start);
    let (base, prefix) = (subnet.network(), subnet.prefix());
    let host_bits = 32 - prefix;
    let last_usable = u32::from(base) + (1u32 << host_bits) - 2; // last host before broadcast
    if next > last_usable {
        return None;
    }
    Some(Ipv4Addr::from(next.to_be_bytes()))
}
#[derive(Debug)]
pub struct NetworkManager {
    map: HashMap<String, NetworkSpec>,
    /// key: network_name; value: bridge interface
    network_interface: HashMap<String, String>,
    /// key: network_name; value: allocated subnet and gateway (Podman-style from subnet pool)
    network_subnets: HashMap<String, NetworkSubnet>,
    /// key: (container_id, network_name) -> allocated container IP (from subnet, not 127.0.0.1)
    container_ips: HashMap<(String, String), Ipv4Addr>,
    /// key: network_name; value: last assigned container IP in that subnet
    network_next_ip: HashMap<String, Ipv4Addr>,
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
            network_subnets: HashMap::new(),
            container_ips: HashMap::new(),
            network_next_ip: HashMap::new(),
        }
    }

    /// Must be called before the container is started.
    pub fn allocate_container_ip(
        &mut self,
        network_name: &str,
        container_id: &str,
    ) -> Result<Ipv4Addr> {
        let (subnet, gateway) = self
            .network_subnets
            .get(network_name)
            .map(|s| (s.subnet, s.gateway))
            .ok_or_else(|| anyhow!("No subnet for network {}", network_name))?;
        let last = self.network_next_ip.get(network_name).copied();
        let ip = next_container_ip_in_subnet(&subnet, gateway, last)
            .ok_or_else(|| anyhow!("No free IP in subnet for network {}", network_name))?;
        self.network_next_ip.insert(network_name.to_string(), ip);
        self.container_ips
            .insert((container_id.to_string(), network_name.to_string()), ip);
        Ok(ip)
    }

    /// Get the allocated IP for a container on a network (for use in netavark static_ips).
    pub fn get_container_ip(&self, container_id: &str, network_name: &str) -> Option<Ipv4Addr> {
        self.container_ips
            .get(&(container_id.to_string(), network_name.to_string()))
            .copied()
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

        let (subnet_addr, gateway_addr, prefix) = self
            .network_subnets
            .get(network_name)
            .map(|s| (s.subnet.network(), s.gateway, s.subnet.prefix()))
            .ok_or_else(|| anyhow!("No subnet allocated for network {}", network_name))?;

        let conf = CliNetworkConfig::from_subnet_gateway(
            network_name,
            interface,
            subnet_addr,
            gateway_addr,
            prefix,
        );

        let conf_value = serde_json::to_value(conf).expect("Failed to parse network config");

        let mut conf_path = PathBuf::from(STD_CONF_PATH);
        conf_path.push(BRIDGE_CONF);
        if let Some(parent) = conf_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
        }

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
        self.allocate_interface()?;
        // allocate subnet per network (Podman-compose style: first free /24 from subnet pools)
        self.allocate_subnets()?;
        Ok(())
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

    fn allocate_subnets(&mut self) -> Result<()> {
        let pools = default_subnet_pools();
        let mut used: Vec<Ipv4Network> = Vec::new();
        for (network_name, spec) in &self.map.clone() {
            if matches!(spec.driver, Some(Bridge)) {
                let (subnet, gateway) = get_free_ipv4_subnet(&used, &pools)
                    .ok_or_else(|| anyhow!("could not find free subnet from subnet pools"))?;
                used.push(subnet);
                self.network_subnets
                    .insert(network_name.clone(), NetworkSubnet { subnet, gateway });
            } else {
                todo!()
            }
        }
        Ok(())
    }

    pub(crate) fn after_container_started(&self, runner: ContainerRunner) -> Result<()> {
        let container_id = runner.id();
        if runner
            .ip()
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .is_ipv4()
        {
            let alias = vec![runner.id()];
            let mut networks = HashMap::new();
            let mut network_info = HashMap::new();
            for (network_name, _) in self.map.clone() {
                let network_se = match self.network_service.get(&network_name) {
                    Some(network) => network,
                    None => continue,
                };
                for (_container, container_spec) in network_se {
                    let Some(container_name) = container_spec.container_name.as_ref() else {
                        continue;
                    };
                    if *container_name == container_id {
                        let container_ip = self
                            .get_container_ip(&container_id, &network_name)
                            .map(IpAddr::V4)
                            .ok_or_else(|| {
                                anyhow!(
                                    "[container {}] No allocated IP for network {}",
                                    container_id,
                                    network_name
                                )
                            })?;
                        let (gateway, subnet_net) = self
                            .network_subnets
                            .get(&network_name)
                            .map(|s| {
                                (
                                    s.gateway,
                                    IpNet::new(IpAddr::V4(s.subnet.network()), s.subnet.prefix())
                                        .unwrap(),
                                )
                            })
                            .ok_or_else(|| anyhow!("No subnet for network {}", network_name))?;
                        let network_opts = PerNetworkOptions {
                            aliases: Some(alias.clone()),
                            interface_name: self
                                .network_interface
                                .get(&network_name)
                                .unwrap_or(&"vethcni0".to_string())
                                .clone(),
                            static_ips: Some(vec![container_ip]),
                            static_mac: None,
                            options: None,
                        };
                        let network_interface = Some(
                            self.network_interface
                                .get(&network_name)
                                .unwrap_or(&"vethcni0".to_string())
                                .clone(),
                        );
                        let network = Network {
                            dns_enabled: true,
                            driver: "bridge".to_string(),
                            id: network_name.clone(),
                            internal: false,
                            ipv6_enabled: false,
                            name: network_name.clone(),
                            network_interface,
                            options: None,
                            ipam_options: None,
                            subnets: Some(vec![Subnet {
                                gateway: Some(IpAddr::V4(gateway)),
                                lease_range: None,
                                subnet: subnet_net,
                            }]),
                            routes: None,
                            network_dns_servers: Some(vec![]),
                        };

                        networks.insert(network_name.clone(), network_opts);
                        network_info.insert(network_name, network);
                        break;
                    }
                }
            }
            let opts = NetworkOptions {
                container_id: runner.id(),
                container_name: runner.id(),
                container_hostname: None,
                networks,
                network_info,
                port_mappings: None,
                dns_servers: None,
            };

            let pid = runner
                .get_container_state()?
                .pid
                .ok_or_else(|| anyhow!("[container {}] PID not found", runner.id()))?;
            let netns_path = format!("/proc/{pid}/ns/net");
            if !Path::new(&netns_path).exists() {
                return Err(anyhow!(
                    "[container {}] netns path not found: {netns_path}",
                    runner.id()
                ));
            }
            let setup = Setup::new(netns_path);
            let json_path = create_tmp_netavark_json(&opts, &runner.id())?;
            let config_dir = default_netavark_config_dir();
            fs::create_dir_all(PathBuf::from(&config_dir))?;
            setup
                .exec(
                    Some(json_path),
                    Some(config_dir),
                    None,
                    default_aardvark_bin()?,
                    None,
                    false,
                )
                .map_err(|e| anyhow!("[container {}] netavark setup failed: {e}", runner.id()))?;
            clean_tmp_netavark_json(&runner.id())?;
        } else {
            return Err(anyhow!("Unsupported ipv6 type"));
        }

        Ok(())
    }

    pub fn clean_up(network_name_space: String, id: String) -> Result<()> {
        let teardown = Teardown::new(network_name_space);
        let config_dir = default_netavark_config_dir();
        let mut json_path = std::env::temp_dir();
        json_path.push("rkl-netavark");
        json_path.push(format!("{}.network.json", id));
        teardown
            .exec(
                Some(json_path.into_os_string()),
                Some(config_dir),
                None,
                default_aardvark_bin()?,
                None,
                false,
            )
            .map_err(|e| anyhow!("[container {}] netavark teardown failed: {e}", id))?;
        Ok(())
    }
}
// in the future , add dns server
/// create resolv.conf file
/// save base resolv.conf file
pub fn create_resolv_conf() -> Result<()> {
    // todo perfect future
    let contect = "search rkl.internal\nnameserver 172.17.0.1\n";
    let mut resolv_path = std::env::temp_dir();
    resolv_path.push("rkl-netavark");
    resolv_path.push("resolv.conf");
    if let Some(parent) = resolv_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(resolv_path)?;
    file.write_all(contect.as_bytes())?;
    Ok(())
}

/// add resolv.conf to container
pub fn add_resolv_conf(runner: &mut ContainerRunner) {
    let mut host_path = std::env::temp_dir();
    host_path.push("rkl-netavark");
    host_path.push("resolv.conf");
    let host_path = host_path.into_os_string().into_string().unwrap();
    let container_path = "/etc/resolv.conf".to_string();

    runner.add_mounts(vec![Mount {
        host_path,
        container_path,
        readonly: true,
        ..Default::default()
    }]);
}

use lazy_static::lazy_static;

lazy_static! {
    static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime for network manager.");
}

#[allow(unused)]
fn spawn<F, T>(f: F) -> tokio::task::JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    RUNTIME.spawn(f)
}

fn create_tmp_netavark_json(opts: &NetworkOptions, id: &str) -> Result<OsString> {
    let mut json_path = std::env::temp_dir();
    json_path.push("rkl-netavark");
    json_path.push(format!("{}.network.json", id));
    if let Some(parent) = json_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json_str = serde_json::to_string(&opts)?;
    let mut json_file = std::fs::File::create(&json_path)?;
    json_file.write_all(json_str.as_bytes())?;
    Ok(json_path.into_os_string())
}
fn clean_tmp_netavark_json(id: &str) -> Result<()> {
    let mut json_path = std::env::temp_dir();
    json_path.push("rkl-netavark");
    json_path.push(format!("{}.network.json", id));
    if let Some(parent) = json_path.parent() {
        fs::create_dir_all(parent)?;
    }
    std::fs::remove_file(&json_path)?;

    Ok(())
}
