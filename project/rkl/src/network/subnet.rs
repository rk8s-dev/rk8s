#![allow(dead_code)]
#![allow(unused_imports)]
use std::fs;
use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use ipnetwork::{Ipv4Network, Ipv6Network};
use log::{error, info, warn};

use libnetwork::{
    config::NetworkConfig,
    subnet::{make_subnet_key, parse_subnet_key, write_subnet_file},
};

/// Read IPv4 CIDRs from subnet file
pub fn read_cidrs_from_subnet_file(path: &str, cidr_key: &str) -> Vec<Ipv4Network> {
    let mut cidrs = Vec::new();
    if !Path::new(path).exists() {
        return cidrs;
    }

    match dotenvy::from_path_iter(path) {
        Ok(iter) => {
            for (key, value) in iter.flatten() {
                if key == cidr_key {
                    for s in value.split(',') {
                        match Ipv4Network::from_str(s.trim()) {
                            Ok(cidr) => cidrs.push(cidr),
                            Err(e) => error!(
                                "Couldn't parse previous {cidr_key} from subnet file at {path}: {e}"
                            ),
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("Couldn't fetch previous {cidr_key} from subnet file at {path}: {e}");
        }
    }

    cidrs
}

/// Read single IPv6 CIDR from subnet file
pub fn read_ip6_cidr_from_subnet_file(path: &str, cidr_key: &str) -> Option<Ipv6Network> {
    let cidrs = read_ip6_cidrs_from_subnet_file(path, cidr_key);
    match cidrs.len() {
        0 => {
            warn!("no subnet found for key: {cidr_key} in file: {path}");
            None
        }
        1 => Some(cidrs[0]),
        _ => {
            error!(
                "error reading subnet: more than 1 entry found for key: {cidr_key} in file: {path}"
            );
            None
        }
    }
}

/// Read IPv6 CIDRs from subnet file
pub fn read_ip6_cidrs_from_subnet_file(path: &str, cidr_key: &str) -> Vec<Ipv6Network> {
    let mut cidrs = Vec::new();
    if !Path::new(path).exists() {
        return cidrs;
    }

    match dotenvy::from_path_iter(path) {
        Ok(iter) => {
            for (key, value) in iter.flatten() {
                if key == cidr_key {
                    for s in value.split(',') {
                        match Ipv6Network::from_str(s.trim()) {
                            Ok(cidr) => cidrs.push(cidr),
                            Err(e) => error!(
                                "Couldn't parse previous {cidr_key} from subnet file at {path}: {e}"
                            ),
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("Couldn't fetch previous {cidr_key} from subnet file at {path}: {e}");
        }
    }

    cidrs
}

/// Subnet configuration receiver
/// This will be called when receiving subnet.env configuration from rks
pub struct SubnetReceiver {
    pub subnet_file_path: String,
}

impl SubnetReceiver {
    pub fn new(subnet_file_path: String) -> Self {
        Self { subnet_file_path }
    }

    /// Handle received subnet configuration from rks
    /// This function will be called when rks sends subnet.env configuration
    pub async fn handle_subnet_config(
        &self,
        config: &NetworkConfig,
        ip_masq: bool,
        sn4: Option<Ipv4Network>,
        sn6: Option<Ipv6Network>,
        mtu: u32,
    ) -> Result<()> {
        info!(
            "Received subnet configuration from rks: IPv4={sn4:?}, IPv6={sn6:?}, MTU={mtu}, IP_MASQ={ip_masq}"
        );

        // Validate the configuration before applying
        self.validate_subnet_config(config, sn4, sn6)?;

        // Write the configuration to file
        write_subnet_file(&self.subnet_file_path, config, ip_masq, sn4, sn6, mtu)?;

        info!(
            "Subnet configuration applied successfully to: {}",
            self.subnet_file_path
        );
        Ok(())
    }

    /// Receive subnet configurations from rks via QUIC
    /// This method will be called by the main NetworkReceiver QUIC loop
    pub async fn receive_from_rks(&self) -> Result<()> {
        // This method is now integrated with the main NetworkReceiver QUIC communication
        // The actual QUIC communication is handled by NetworkReceiver.quic_communication_loop()
        // When subnet configurations are received, they will be passed to handle_subnet_config()
        info!("Subnet receiver is ready to receive configurations from rks");
        Ok(())
    }

    /// Validate received subnet configuration before applying
    pub fn validate_subnet_config(
        &self,
        config: &NetworkConfig,
        sn4: Option<Ipv4Network>,
        sn6: Option<Ipv6Network>,
    ) -> Result<()> {
        if config.enable_ipv4 && sn4.is_none() {
            return Err(anyhow::anyhow!(
                "IPv4 is enabled but no IPv4 subnet provided"
            ));
        }

        if config.enable_ipv6 && sn6.is_none() {
            return Err(anyhow::anyhow!(
                "IPv6 is enabled but no IPv6 subnet provided"
            ));
        }

        if let Some(network) = config.network
            && let Some(subnet) = sn4
            && !network.contains(subnet.network())
        {
            return Err(anyhow::anyhow!(
                "IPv4 subnet {} is not within network {}",
                subnet,
                network
            ));
        }

        if let Some(network) = config.ipv6_network
            && let Some(subnet) = sn6
            && !network.contains(subnet.network())
        {
            return Err(anyhow::anyhow!(
                "IPv6 subnet {} is not within network {}",
                subnet,
                network
            ));
        }

        Ok(())
    }

    /// Get current subnet configuration from file
    pub fn get_current_config(&self) -> Result<Option<(Ipv4Network, Option<Ipv6Network>)>> {
        if !Path::new(&self.subnet_file_path).exists() {
            return Ok(None);
        }

        let ipv4_cidrs = read_cidrs_from_subnet_file(&self.subnet_file_path, "RKL_SUBNET");
        let ipv6_cidr = read_ip6_cidr_from_subnet_file(&self.subnet_file_path, "RKL_IPV6_SUBNET");

        match ipv4_cidrs.len() {
            0 => Ok(None),
            1 => Ok(Some((ipv4_cidrs[0], ipv6_cidr))),
            _ => Err(anyhow::anyhow!(
                "Multiple IPv4 subnets found in subnet file: {}",
                self.subnet_file_path
            )),
        }
    }

    /// Clear subnet configuration file
    pub fn clear_config(&self) -> Result<()> {
        if Path::new(&self.subnet_file_path).exists() {
            fs::remove_file(&self.subnet_file_path)?;
            info!(
                "Cleared subnet configuration file: {}",
                self.subnet_file_path
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parse_subnet_key() {
        // IPv4 only
        let result = parse_subnet_key("10.0.1.0-24");
        assert!(result.is_some());
        let (ipv4, ipv6) = result.unwrap();
        assert_eq!(ipv4.to_string(), "10.0.1.0/24");
        assert!(ipv6.is_none());

        // IPv4 + IPv6
        let result = parse_subnet_key("10.0.1.0-24&fc00::-64");
        assert!(result.is_some());
        let (ipv4, ipv6) = result.unwrap();
        assert_eq!(ipv4.to_string(), "10.0.1.0/24");
        assert_eq!(ipv6.unwrap().to_string(), "fc00::/64");
    }

    #[test]
    fn test_make_subnet_key() {
        let ipv4: Ipv4Network = "10.0.1.0/24".parse().unwrap();
        let ipv6: Ipv6Network = "fc00::/64".parse().unwrap();

        // IPv4 only
        let key = make_subnet_key(&ipv4, None);
        assert_eq!(key, "10.0.1.0-24");

        // IPv4 + IPv6
        let key = make_subnet_key(&ipv4, Some(&ipv6));
        assert_eq!(key, "10.0.1.0-24&fc00::-64");
    }

    #[test]
    fn test_write_subnet_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("subnet.env");

        let config = NetworkConfig {
            enable_ipv4: true,
            enable_ipv6: false,
            network: Some("10.0.0.0/16".parse().unwrap()),
            ..Default::default()
        };

        let subnet: Ipv4Network = "10.0.1.0/24".parse().unwrap();

        write_subnet_file(&file_path, &config, true, Some(subnet), None, 1500).unwrap();

        let contents = fs::read_to_string(&file_path).unwrap();
        assert!(contents.contains("RKL_NETWORK=10.0.0.0/16"));
        assert!(contents.contains("RKL_SUBNET=10.0.1.0/24"));
        assert!(contents.contains("RKL_MTU=1500"));
        assert!(contents.contains("RKL_IPMASQ=true"));
    }
}
