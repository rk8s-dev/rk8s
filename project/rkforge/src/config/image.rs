use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use std::{fs, path::PathBuf};

pub static CONFIG: Lazy<Config> =
    Lazy::new(|| Config::new().expect("Failed to initialize configuration"));

static REGISTRY: &str = "47.79.87.161:8968";
static ROOT_PATH: &str = "/var/lib/rkforge";

/// Configuration for rkforge build
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

        let (layers_store_root, build_dir, metadata_dir) = if is_root {
            let root_dir = PathBuf::from(ROOT_PATH);
            (
                root_dir.join("layers"),
                root_dir.join("build"),
                root_dir.join("metadata"),
            )
        } else {
            let data_dir = dirs::data_dir()
                .context("Failed to get user data directory")?
                .join("rk8s");
            (
                data_dir.join("layers"),
                data_dir.join("build"),
                data_dir.join("metadata"),
            )
        };

        fs::create_dir_all(&layers_store_root).with_context(|| {
            format!(
                "Failed to create layers directory at {:?}",
                layers_store_root
            )
        })?;
        fs::create_dir_all(&build_dir)
            .with_context(|| format!("Failed to create build directory at {:?}", build_dir))?;
        fs::create_dir_all(&metadata_dir).with_context(|| {
            format!("Failed to create metadata directory at {:?}", metadata_dir)
        })?;

        Ok(Self {
            layers_store_root,
            build_dir,
            metadata_dir,
            default_registry: String::from(REGISTRY),
            is_root,
        })
    }
}
