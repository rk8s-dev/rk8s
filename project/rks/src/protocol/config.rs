use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub struct Config {
    // ip:port of RKS
    pub addr: String,
    // Xline endpoints
    pub xline_endpoints: Vec<String>,
}

pub fn load_config(path: &str) -> Result<Config> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read config from {}", path))?;
    let cfg: Config = serde_yaml::from_str(&content).context("Failed to parse YAML config")?;
    Ok(cfg)
}
