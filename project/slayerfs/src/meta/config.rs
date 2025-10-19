//! SlayerFS configuration management
//!
//! Database connection configuration supporting SQLite, PostgreSQL and Etcd

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

/// SlayerFS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,

    /// Cache configuration (optional, uses backend-specific defaults if not specified)
    #[serde(default)]
    pub cache: CacheConfig,
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

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Cache capacity settings
    #[serde(default)]
    pub capacity: CacheCapacityConfig,

    /// Cache TTL settings
    #[serde(default)]
    pub ttl: CacheTtlConfig,

    /// Whether cache is enabled (default: true)
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
}

/// Cache capacity configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheCapacityConfig {
    /// File attributes cache capacity
    #[serde(default = "default_attr_capacity")]
    pub attr: usize,

    /// Directory entry cache capacity
    #[serde(default = "default_dentry_capacity")]
    pub dentry: usize,

    /// Path resolution cache capacity
    #[serde(default = "default_path_capacity")]
    pub path: usize,

    /// Reverse path lookup cache capacity
    #[serde(default = "default_inode_to_path_capacity")]
    pub inode_to_path: usize,

    /// Directory content (readdir) cache capacity
    #[serde(default = "default_readdir_capacity")]
    pub readdir: usize,
}

/// Cache TTL configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheTtlConfig {
    /// File attributes cache TTL (in seconds)
    #[serde(default, with = "duration_serde")]
    pub attr_ttl: Duration,

    /// Directory entry cache TTL (in seconds)
    #[serde(default, with = "duration_serde")]
    pub dentry_ttl: Duration,

    /// Path resolution cache TTL (in seconds)
    #[serde(default, with = "duration_serde")]
    pub path_ttl: Duration,

    /// Reverse path lookup cache TTL (in seconds)
    #[serde(default, with = "duration_serde")]
    pub inode_to_path_ttl: Duration,

    /// Directory content (readdir) cache TTL (in seconds)
    #[serde(default, with = "duration_serde")]
    pub readdir_ttl: Duration,
}

// Default value functions
fn default_cache_enabled() -> bool {
    true
}

fn default_attr_capacity() -> usize {
    10000
}

fn default_dentry_capacity() -> usize {
    10000
}

fn default_path_capacity() -> usize {
    5000
}

fn default_inode_to_path_capacity() -> usize {
    5000
}

fn default_readdir_capacity() -> usize {
    1000
}

impl Default for CacheCapacityConfig {
    fn default() -> Self {
        Self {
            attr: default_attr_capacity(),
            dentry: default_dentry_capacity(),
            path: default_path_capacity(),
            inode_to_path: default_inode_to_path_capacity(),
            readdir: default_readdir_capacity(),
        }
    }
}

impl CacheTtlConfig {
    /// Get default TTL based on database backend type
    pub fn for_backend(backend: &str) -> Self {
        match backend {
            "sqlite" => Self::for_sqlite(),
            "postgres" => Self::for_postgres(),
            "etcd" => Self::for_etcd(),
            _ => Self::for_sqlite(),
        }
    }

    /// SQLite backend defaults (10s TTL for local database)
    pub fn for_sqlite() -> Self {
        Self {
            attr_ttl: Duration::from_secs(10),
            dentry_ttl: Duration::from_secs(8),
            path_ttl: Duration::from_secs(10),
            inode_to_path_ttl: Duration::from_secs(10),
            readdir_ttl: Duration::from_secs(5),
        }
    }

    /// PostgreSQL backend defaults (500ms TTL for network latency)
    pub fn for_postgres() -> Self {
        Self {
            attr_ttl: Duration::from_millis(500),
            dentry_ttl: Duration::from_millis(300),
            path_ttl: Duration::from_millis(500),
            inode_to_path_ttl: Duration::from_millis(500),
            readdir_ttl: Duration::from_millis(300),
        }
    }

    /// Etcd backend defaults (100ms TTL for distributed consistency)
    pub fn for_etcd() -> Self {
        Self {
            attr_ttl: Duration::from_millis(100),
            dentry_ttl: Duration::from_millis(50),
            path_ttl: Duration::from_millis(100),
            inode_to_path_ttl: Duration::from_millis(100),
            readdir_ttl: Duration::from_millis(50),
        }
    }

    /// Check if this is a zero/default TTL config
    pub fn is_zero(&self) -> bool {
        self.attr_ttl.is_zero()
            && self.dentry_ttl.is_zero()
            && self.path_ttl.is_zero()
            && self.inode_to_path_ttl.is_zero()
            && self.readdir_ttl.is_zero()
    }
}

impl Default for CacheTtlConfig {
    fn default() -> Self {
        // Return zero duration, will be replaced by backend-specific defaults
        Self {
            attr_ttl: Duration::ZERO,
            dentry_ttl: Duration::ZERO,
            path_ttl: Duration::ZERO,
            inode_to_path_ttl: Duration::ZERO,
            readdir_ttl: Duration::ZERO,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            capacity: CacheCapacityConfig::default(),
            ttl: CacheTtlConfig::default(),
            enabled: true,
        }
    }
}

impl CacheConfig {
    /// Validate cache configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled {
            if self.capacity.attr == 0 {
                return Err("attr cache capacity must be > 0".into());
            }
            if self.capacity.dentry == 0 {
                return Err("dentry cache capacity must be > 0".into());
            }
            if self.capacity.path == 0 {
                return Err("path cache capacity must be > 0".into());
            }
            if self.capacity.inode_to_path == 0 {
                return Err("inode_to_path cache capacity must be > 0".into());
            }
        }
        Ok(())
    }
}

/// Custom serde module for Duration (supports seconds as float/int)
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let secs = duration.as_secs_f64();
        serializer.serialize_f64(secs)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Ok(Duration::from_secs_f64(value))
    }
}
