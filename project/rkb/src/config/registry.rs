use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use std::{fs, path::PathBuf};

pub static CONFIG: Lazy<Config> =
    Lazy::new(|| Config::new().expect("Failed to initialize configuration"));

pub static BLOBS: &str = "blobs/sha256";
pub static DNS_CONFIG: &str = "/etc/resolv.conf";
static REGISTRY: &str = "47.79.87.161:8968";
static LAYER_PATH: &str = "/var/lib/rkb/layers";
static BUILD_PATH: &str = "/var/lib/rkb/build";
static METADATA_PATH: &str = "/var/lib/rkb/metadata";

pub static BIND_MOUNTS: [&str; 3] = ["/dev", "/proc", "/sys"];

#[derive(Debug)]
pub struct Config {
    pub layers_store_root: PathBuf,
    pub build_dir: PathBuf,
    pub metadata_dir: PathBuf,
    pub default_registry: String,
    pub is_root: bool,
}

impl Config {
    pub fn new() -> Result<Self> {
        let is_root = nix::unistd::getuid().is_root();

        let (layers_store_root, build_dir) = if is_root {
            (PathBuf::from(LAYER_PATH), PathBuf::from(BUILD_PATH))
        } else {
            let data_dir = dirs::data_dir()
                .context("Failed to get user data directory")?
                .join("rk8s");
            (data_dir.join("layers"), data_dir.join("build"))
        };

        fs::create_dir_all(&layers_store_root).with_context(|| {
            format!("Failed to create layers store root at {layers_store_root:?}")
        })?;
        fs::create_dir_all(&build_dir)
            .with_context(|| format!("Failed to create build directory at {build_dir:?}"))?;
        fs::create_dir_all(METADATA_PATH)
            .with_context(|| format!("Failed to create metadata directory at {METADATA_PATH:?}"))?;

        Ok(Self {
            layers_store_root,
            build_dir: build_dir.clone(),
            metadata_dir: METADATA_PATH.into(),
            default_registry: String::from(REGISTRY),
            is_root,
        })
    }
}
