use crate::config::NetworkConfig;
use anyhow::{Context, Result};
use ipnetwork::{Ipv4Network, Ipv6Network};
use lazy_static::lazy_static;
use log::info;
use regex::Regex;
use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;

lazy_static! {
    static ref SUBNET_REGEX: Regex =
        Regex::new(r"(\d+\.\d+\.\d+\.\d+)-(\d+)(?:&([a-f\d:]+)-(\d+))?").unwrap();
}

/// Parse subnet key into IPv4 and optional IPv6 networks
pub fn parse_subnet_key(s: &str) -> Option<(Ipv4Network, Option<Ipv6Network>)> {
    if let Some(caps) = SUBNET_REGEX.captures(s) {
        let ipv4: Ipv4Addr = caps[1].parse().ok()?;
        let ipv4_prefix: u8 = caps[2].parse().ok()?;
        let ipv4_net = Ipv4Network::new(ipv4, ipv4_prefix).ok()?;

        let ipv6_net = if let (Some(ipv6_str), Some(prefix_str)) = (caps.get(3), caps.get(4)) {
            let ipv6: Ipv6Addr = ipv6_str.as_str().parse().ok()?;
            let prefix: u8 = prefix_str.as_str().parse().ok()?;
            Some(Ipv6Network::new(ipv6, prefix).ok()?)
        } else {
            None
        };

        Some((ipv4_net, ipv6_net))
    } else {
        None
    }
}

/// Create subnet key from IPv4 and optional IPv6 networks
pub fn make_subnet_key(sn4: &Ipv4Network, sn6: Option<&Ipv6Network>) -> String {
    match sn6 {
        Some(v6) => format!(
            "{}&{}",
            sn4.to_string().replace("/", "-"),
            v6.to_string().replace("/", "-")
        ),
        None => sn4.to_string().replace("/", "-"),
    }
}

/// Write subnet configuration to file
pub fn write_subnet_file<P: AsRef<Path>>(
    path: P,
    config: &NetworkConfig,
    ip_masq: bool,
    mut sn4: Option<Ipv4Network>,
    mut sn6: Option<Ipv6Network>,
    mtu: u32,
) -> Result<()> {
    let path = path.as_ref();
    let (dir, name) = (
        path.parent().context("Missing parent directory")?,
        path.file_name().context("Missing file name")?,
    );
    fs::create_dir_all(dir)?;

    let temp_file = dir.join(format!(".{}", name.to_string_lossy()));
    let mut contents = String::new();

    if config.enable_ipv4
        && let Some(ref mut net) = sn4
    {
        contents += &format!("RKL_NETWORK={}\n", config.network.unwrap());
        contents += &format!("RKL_SUBNET={}/{}\n", net.ip(), net.prefix());
    }

    if config.enable_ipv6
        && let Some(ref mut net) = sn6
    {
        contents += &format!("RKL_IPV6_NETWORK={}\n", config.ipv6_network.unwrap());
        contents += &format!("RKL_IPV6_SUBNET={}/{}\n", net.ip(), net.prefix());
    }

    contents += &format!("RKL_MTU={mtu}\n");
    contents += &format!("RKL_IPMASQ={ip_masq}\n");

    fs::write(&temp_file, contents)?;
    fs::rename(&temp_file, path)?;

    info!("Subnet configuration written to: {}", path.display());
    Ok(())
}
