use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub struct Config {
    // ip:port of RKS
    pub addr: String,
    // Xline endpoints
    pub xline_config: XlineConfig,
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

pub fn load_config(path: &str) -> Result<Config> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read config from {path}"))?;
    let cfg: Config = serde_yaml::from_str(&content).context("Failed to parse YAML config")?;
    Ok(cfg)
}
