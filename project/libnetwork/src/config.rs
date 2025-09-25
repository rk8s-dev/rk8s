#![allow(dead_code)]
use crate::ip;
use anyhow::{Context, Result};
use ipnetwork::{Ipv4Network, Ipv6Network};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value as JsonValue};
use std::net::{Ipv4Addr, Ipv6Addr};
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NetworkConfig {
    #[serde(rename = "EnableIPv4", default)]
    pub enable_ipv4: bool,
    #[serde(rename = "EnableIPv6", default)]
    pub enable_ipv6: bool,
    #[serde(rename = "EnableNFTables", default)]
    pub enable_nftables: bool,

    #[serde(rename = "Network", default)]
    pub network: Option<Ipv4Network>,
    #[serde(rename = "IPv6Network", default)]
    pub ipv6_network: Option<Ipv6Network>,

    #[serde(rename = "SubnetMin", default)]
    pub subnet_min: Option<Ipv4Addr>,
    #[serde(rename = "SubnetMax", default)]
    pub subnet_max: Option<Ipv4Addr>,
    #[serde(rename = "IPv6SubnetMin", default)]
    pub ipv6_subnet_min: Option<Ipv6Addr>,
    #[serde(rename = "IPv6SubnetMax", default)]
    pub ipv6_subnet_max: Option<Ipv6Addr>,

    #[serde(rename = "SubnetLen", default)]
    pub subnet_len: u8,
    #[serde(rename = "IPv6SubnetLen", default)]
    pub ipv6_subnet_len: u8,

    // populated after parsing
    #[serde(skip)]
    pub backend_type: String,
    #[serde(rename = "Backend", skip_serializing_if = "Option::is_none")]
    pub backend: Option<JsonValue>,
}

#[derive(Deserialize)]
struct BackendType {
    #[serde(rename = "Type")]
    pub r#type: String,
}

fn parse_backend_type(be: &Option<JsonValue>) -> Result<String> {
    if let Some(val) = be {
        // empty raw means default
        if val.is_null() {
            return Ok("hostgw".into());
        }
        let bt: BackendType =
            serde_json::from_value(val.clone()).context("decoding Backend property of config")?;
        Ok(bt.r#type)
    } else {
        Ok("hostgw".into())
    }
}

/// Parse network configuration from JSON string
pub fn parse_network_config(s: &str) -> Result<NetworkConfig> {
    let mut cfg: NetworkConfig = serde_json::from_str(s).context("parsing NetworkConfig JSON")?;
    // default enable ipv4
    cfg.enable_ipv4 = true;
    cfg.backend_type = parse_backend_type(&cfg.backend)?;
    Ok(cfg)
}

/// Validate and adjust network configuration
pub fn validate_network_config(cfg: &mut NetworkConfig) -> Result<()> {
    if cfg.enable_ipv4 {
        let net = cfg
            .network
            .with_context(|| "please define a correct Network parameter in the flannel config")?;
        let prefix = net.prefix();

        // determine subnet length
        if cfg.subnet_len > 0 {
            if cfg.subnet_len > 30 {
                anyhow::bail!("SubnetLen must be less than /31");
            }
            if cfg.subnet_len < prefix + 2 {
                anyhow::bail!("network must be able to accommodate at least four subnets");
            }
        } else if prefix > 28 {
            anyhow::bail!("network is too small. Minimum useful network prefix is /28");
        } else if prefix <= 22 {
            cfg.subnet_len = 24;
        } else {
            cfg.subnet_len = prefix + 2;
        }

        let size = 1u32 << (32 - cfg.subnet_len);
        // SubnetMin
        let min_ip = if let Some(min) = cfg.subnet_min {
            if !net.contains(min) {
                anyhow::bail!("SubnetMin is not in the range of the Network");
            }
            min
        } else {
            ip::AddIP::add(net.ip(), size)
        };
        cfg.subnet_min = Some(min_ip);

        // SubnetMax
        let max_ip = if let Some(max) = cfg.subnet_max {
            if !net.contains(max) {
                anyhow::bail!("SubnetMax is not in the range of the Network");
            }
            max
        } else {
            let nxt = ip::AddIP::add(net.broadcast(), 1);
            ip::SubIP::sub(nxt, size)
        };
        cfg.subnet_max = Some(max_ip);

        // boundary checks
        let mask = u32::MAX << (32 - cfg.subnet_len);
        let min_u = u32::from(min_ip);
        if min_u != (min_u & mask) {
            anyhow::bail!("SubnetMin is not on a SubnetLen boundary: {min_ip}");
        }
        let max_u = u32::from(max_ip);
        if max_u != (max_u & mask) {
            anyhow::bail!("SubnetMax is not on a SubnetLen boundary: {max_ip}");
        }
    }

    if cfg.enable_ipv6 {
        let net6 = cfg.ipv6_network.as_ref().with_context(
            || "please define a correct IPv6Network parameter in the flannel config",
        )?;
        let prefix6 = net6.prefix();

        if cfg.ipv6_subnet_len > 0 {
            if cfg.ipv6_subnet_len > 126 {
                anyhow::bail!("SubnetLen must be less than /127");
            }
            if cfg.ipv6_subnet_len < (prefix6 + 2) {
                anyhow::bail!("network must be able to accommodate at least four subnets");
            }
        } else if prefix6 > 124 {
            anyhow::bail!("IPv6Network is too small. Minimum useful network prefix is /124");
        } else if prefix6 <= 62 {
            cfg.ipv6_subnet_len = 64;
        } else {
            cfg.ipv6_subnet_len = prefix6 + 2;
        }

        let size6: u128 = 1u128 << (128 - cfg.ipv6_subnet_len);

        // SubnetMin
        let min6 = if let Some(min) = cfg.ipv6_subnet_min {
            if !net6.contains(min) {
                anyhow::bail!("IPv6SubnetMin is not in the range of the IPv6Network");
            }
            min
        } else {
            ip::AddIP::add(net6.ip(), size6)
        };
        cfg.ipv6_subnet_min = Some(min6);

        // SubnetMax
        let max6 = if let Some(max) = cfg.ipv6_subnet_max {
            if !net6.contains(max) {
                anyhow::bail!("IPv6SubnetMax is not in the range of the IPv6Network");
            }
            max
        } else {
            let nxt = ip::AddIP::add(net6.broadcast(), 1u128);
            ip::SubIP::sub(nxt, size6)
        };
        cfg.ipv6_subnet_max = Some(max6);

        // boundary checks
        let mask: u128 = (!0u128) << (128 - cfg.ipv6_subnet_len);

        let min_b = min6.to_bits();
        if min_b & mask != min_b {
            anyhow::bail!("SubnetMin is not on a SubnetLen boundary: {min6}");
        }

        let max_b = max6.to_bits();
        if max_b & mask != max_b {
            anyhow::bail!("SubnetMax is not on a SubnetLen boundary: {max6}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_network_config_ipv4_only() {
        let mut cfg = NetworkConfig {
            enable_ipv4: true,
            enable_ipv6: false,
            enable_nftables: false,
            network: Some("10.0.0.0/16".parse().unwrap()),
            ipv6_network: None,
            subnet_min: None,
            subnet_max: None,
            ipv6_subnet_min: None,
            ipv6_subnet_max: None,
            subnet_len: 24,
            ipv6_subnet_len: 0,
            backend_type: "".into(),
            backend: None,
        };

        validate_network_config(&mut cfg).expect("IPv4 config should pass");
        assert_eq!(cfg.subnet_min.unwrap(), Ipv4Addr::new(10, 0, 1, 0));
        assert_eq!(cfg.subnet_max.unwrap(), Ipv4Addr::new(10, 0, 255, 0));
    }

    #[test]
    fn test_validate_network_config_ipv6_only() {
        let mut cfg = NetworkConfig {
            enable_ipv4: false,
            enable_ipv6: true,
            enable_nftables: false,
            network: None,
            ipv6_network: Some("fc00::/7".parse().unwrap()),
            subnet_min: None,
            subnet_max: None,
            ipv6_subnet_min: None,
            ipv6_subnet_max: None,
            subnet_len: 0,
            ipv6_subnet_len: 64,
            backend_type: "".into(),
            backend: None,
        };

        validate_network_config(&mut cfg).expect("IPv6 config should pass");
        assert_eq!(
            cfg.ipv6_subnet_min.unwrap(),
            "fc00:0:0:1::".parse::<Ipv6Addr>().unwrap()
        );
        assert_eq!(
            cfg.ipv6_subnet_max.unwrap(),
            "fdff:ffff:ffff:ffff::".parse::<Ipv6Addr>().unwrap()
        );
    }
}
