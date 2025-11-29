//! Configuration module for RK8s components (rkl, rkb, rks, distribution).
//!
//! This module provides a unified configuration framework that supports:
//! - Multiple file formats (YAML, TOML, JSON)
//! - Environment variable overrides
//! - Default values
//! - Configuration validation
//!
//! # Usage
//!
//! ```ignore
//! use common::config::{ConfigLoader, FileFormat};
//! use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize)]
//! struct MyConfig {
//!     host: String,
//!     port: u16,
//! }
//!
//! // Load from file
//! let config: MyConfig = ConfigLoader::from_file("config.yaml")?.build()?;
//!
//! // Load from file with env overrides
//! let config: MyConfig = ConfigLoader::from_file("config.yaml")?
//!     .with_env_prefix("APP")
//!     .build()?;
//! ```

mod error;
mod loader;

pub use error::{ConfigError, Result};
pub use loader::{
    ConfigLoader, ConfigValidator, FileFormat, load_user_config, parse_env, parse_env_opt,
    save_user_config, user_config_dir, user_config_path,
};
