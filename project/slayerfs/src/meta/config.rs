//! SlayerFS configuration management
//!
//! Database connection configuration supporting SQLite, PostgreSQL and Etcd

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// SlayerFS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(flatten)]
    pub db_config: DatabaseType,
}

/// Database type enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DatabaseType {
    #[serde(rename = "sqlite")]
    Sqlite {
        #[serde(default = "default_sqlite_url")]
        url: String,
    },
    #[serde(rename = "postgres")]
    Postgres { url: String },
    #[serde(rename = "etcd")]
    Etcd { urls: Vec<String> },
}

fn default_sqlite_url() -> String {
    "sqlite:///tmp/slayerfs/metadata.db".to_string()
}
#[allow(dead_code)]
impl Config {
    /// Load configuration from YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(ConfigError::IoError)?;

        let config: Config =
            serde_yaml::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))?;

        Ok(config)
    }

    /// Load configuration from path, fallback to default paths
    pub fn from_path(backend_path: &Path) -> Result<Self, ConfigError> {
        let config_file = backend_path.join("slayerfs.yml");
        if config_file.exists() {
            return Self::from_file(&config_file);
        }

        Self::from_default_path()
    }

    /// Load configuration from default paths
    pub fn from_default_path() -> Result<Self, ConfigError> {
        let possible_paths = [
            "slayerfs.yml",
            "slayerfs.yaml",
            "config.yml",
            "config.yaml",
            "/etc/slayerfs/config.yml",
        ];

        for path in &possible_paths {
            if std::path::Path::new(path).exists() {
                return Self::from_file(path);
            }
        }

        Err(ConfigError::ConfigNotFound)
    }
}

impl DatabaseConfig {
    /// Get database type string
    pub fn db_type_str(&self) -> &'static str {
        match &self.db_config {
            DatabaseType::Sqlite { .. } => "sqlite",
            DatabaseType::Postgres { .. } => "postgres",
            DatabaseType::Etcd { .. } => "etcd",
        }
    }
}

/// Configuration error types
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Config file not found in default locations")]
    ConfigNotFound,
}
