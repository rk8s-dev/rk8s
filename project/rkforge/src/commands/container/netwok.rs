use anyhow::{Result, anyhow};
use ipnet::IpNet;
use netavark::commands::setup::Setup;
use netavark::commands::teardown::Teardown;
use netavark::network::types::{Network, NetworkOptions, PerNetworkOptions, Subnet};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

fn state_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("RKFORGE_NET_STATE_DIR") {
        return PathBuf::from(v);
    }
    PathBuf::from("/run/rkforge/netavark")
}

fn ensure_state_dir() -> Result<PathBuf> {
    let dir = state_dir();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn state_path_for(id: &str) -> Result<PathBuf> {
    let dir = ensure_state_dir()?;
    Ok(dir.join(format!("{id}.network.json")))
}

fn unique_network_id(prefix: &str, name: &str) -> String {
    // Ensure stable-ish but collision-resistant ids without requiring any daemon/db.
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{prefix}-{name}-{ts}")
}

pub struct RootfulBridgeSpec {
    pub network_name: String,
    pub bridge_name: String,
    pub subnet: IpNet,
    pub gateway: Ipv4Addr,
    /// If set, netavark will configure container with this static IPv4.
    pub static_ipv4: Option<Ipv4Addr>,
    /// Additional DNS search aliases for the container on this network.
    pub aliases: Vec<String>,
}

impl RootfulBridgeSpec {
    pub fn default_single_container_network(container_id: &str) -> Result<Self> {
        let subnet = IpNet::new(IpAddr::V4(Ipv4Addr::new(172, 31, 0, 0)), 16).unwrap();
        let gateway = Ipv4Addr::new(172, 31, 0, 1);
        // Allocate from time-based last octet to reduce collisions; still deterministic enough for single.
        let ip = {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let host = 2 + (ts % 250) as u8;
            Ipv4Addr::new(172, 31, 0, host)
        };
        Ok(Self {
            network_name: unique_network_id("rkforge-single", container_id),
            bridge_name: "rksingle0".to_string(),
            subnet,
            gateway,
            static_ipv4: Some(ip),
            aliases: vec![container_id.to_string()],
        })
    }
}

fn build_netavark_opts(spec: &RootfulBridgeSpec, container_id: &str) -> Result<NetworkOptions> {
    let mut networks = HashMap::new();
    let mut network_info = HashMap::new();

    let static_ips = spec.static_ipv4.map(|ip| vec![IpAddr::V4(ip)]).or(None);

    let per = PerNetworkOptions {
        aliases: if spec.aliases.is_empty() {
            None
        } else {
            Some(spec.aliases.clone())
        },
        interface_name: spec.bridge_name.clone(),
        static_ips,
        static_mac: None,
        options: None,
    };

    let network = Network {
        dns_enabled: true,
        driver: "bridge".to_string(),
        id: spec.network_name.clone(),
        internal: false,
        ipv6_enabled: false,
        name: spec.network_name.clone(),
        network_interface: Some(spec.bridge_name.clone()),
        options: None,
        ipam_options: None,
        subnets: Some(vec![Subnet {
            gateway: Some(IpAddr::V4(spec.gateway)),
            lease_range: None,
            subnet: spec.subnet,
        }]),
        routes: None,
        network_dns_servers: Some(vec![]),
    };

    networks.insert(spec.network_name.clone(), per);
    network_info.insert(spec.network_name.clone(), network);

    Ok(NetworkOptions {
        container_id: container_id.to_string(),
        container_name: container_id.to_string(),
        container_hostname: None,
        networks,
        network_info,
        port_mappings: None,
        dns_servers: None,
    })
}

pub fn setup_rootful_bridge(
    netns_path: &str,
    container_id: &str,
    spec: RootfulBridgeSpec,
) -> Result<Ipv4Addr> {
    if !Path::new(netns_path).exists() {
        return Err(anyhow!("netns path not found: {netns_path}"));
    }

    let opts = build_netavark_opts(&spec, container_id)?;
    let json_path = state_path_for(container_id)?;
    let json = serde_json::to_vec(&opts)?;
    fs::write(&json_path, json)?;

    let setup = Setup::new(netns_path.to_string());
    let config_dir = default_netavark_config_dir();
    fs::create_dir_all(PathBuf::from(&config_dir))?;
    setup
        .exec(
            Some(json_path.clone().into_os_string()),
            Some(config_dir),
            None,
            default_aardvark_bin()?,
            None,
            false,
        )
        .map_err(|e| anyhow!("[{container_id}] netavark setup failed: {e}"))?;

    // Return the configured IP (static by design for sync one-shot flows)
    spec.static_ipv4
        .ok_or_else(|| anyhow!("setup completed but no static_ipv4 was configured"))
}

pub fn teardown_rootful_bridge(netns_path: &str, container_id: &str) -> Result<()> {
    let json_path = state_path_for(container_id)?;
    if !json_path.exists() {
        // Idempotent teardown: if no state exists, treat as already torn down.
        return Ok(());
    }

    let teardown = Teardown::new(netns_path.to_string());
    let config_dir = default_netavark_config_dir();
    teardown
        .exec(
            Some(json_path.clone().into_os_string()),
            Some(config_dir),
            None,
            default_aardvark_bin()?,
            None,
            false,
        )
        .map_err(|e| anyhow!("[{container_id}] netavark teardown failed: {e}"))?;

    let _ = fs::remove_file(&json_path);
    Ok(())
}
