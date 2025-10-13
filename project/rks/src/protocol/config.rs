use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn config() -> &'static Config {
    CONFIG.get().unwrap()
}

#[derive(Debug, Deserialize)]
pub struct Config {
    // ip:port of RKS
    pub addr: String,
    // Xline endpoints
    pub xline_config: XlineConfig,
    // network config
    pub network_config: NetworkConfig,
    // tls connection config
    pub tls_config: TLSConfig,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct XlineConfig {
    pub endpoints: Vec<String>,
    pub prefix: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub subnet_lease_renew_margin: Option<i64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(rename = "Network")]
    pub network: String,

    #[serde(rename = "SubnetMin")]
    pub subnet_min: String,

    #[serde(rename = "SubnetMax")]
    pub subnet_max: String,

    #[serde(rename = "SubnetLen")]
    pub subnet_len: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TLSConfig {
    pub enable: bool,
    pub vault_folder: PathBuf,
}

pub fn load_config(path: &str) -> anyhow::Result<&'static Config> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read config from {path}"))?;
    let cfg: Config = serde_yaml::from_str(&content).context("Failed to parse YAML config")?;
    let cfg = CONFIG.get_or_init(|| cfg);
    Ok(cfg)
}
