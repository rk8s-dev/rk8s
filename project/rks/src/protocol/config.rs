use anyhow::Context;
use either::Either;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn config_ref() -> &'static Config {
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
    // DNS config
    pub dns_config: DnsConfig,
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
    #[serde(default)]
    pub enable: bool,
    #[serde(default = "default_vault_folder")]
    pub vault_folder: PathBuf,
    #[serde(default)]
    pub keep_dangerous_files: bool,
}

fn default_vault_folder() -> PathBuf {
    PathBuf::from("./backend")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    #[serde(rename = "Port")]
    pub port: u16,
}

pub fn load_config(path: &str) -> anyhow::Result<&'static Config> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read config from {path}"))?;
    let cfg: Config = serde_yaml::from_str(&content).context("Failed to parse YAML config")?;
    let cfg = CONFIG.get_or_init(|| cfg);
    Ok(cfg)
}

pub fn ip_or_dns(addr: impl AsRef<str>) -> Either<String, String> {
    let addr = addr.as_ref();
    match addr
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .parse::<SocketAddr>()
    {
        Ok(addr) => Either::Left(addr.ip().to_string()),
        Err(_) => {
            let (host, _) = addr.rsplit_once(':').unwrap_or((addr, ""));
            Either::Right(host.to_string())
        }
    }
}

pub fn to_alt_names_and_ip_sans(
    ip_or_dns: Either<String, String>,
) -> (Option<String>, Option<String>) {
    (ip_or_dns.clone().right(), ip_or_dns.left())
}

pub fn local_alt_names_and_ip_sans() -> (Option<String>, Option<String>) {
    to_alt_names_and_ip_sans(ip_or_dns(&config_ref().addr))
}
