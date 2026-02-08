//! SlayerFS configuration management
//!
//! Database connection configuration supporting SQLite, PostgreSQL and Etcd

use crate::meta::client::MetaClientOptions;
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

    /// Client behaviour configuration (session heartbeat, read-only, etc.)
    #[serde(default)]
    pub client: ClientOptions,
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
    #[serde(rename = "redis")]
    Redis { url: String },
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
            DatabaseType::Redis { .. } => "redis",
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
    pub capacity: CacheCapacity,

    /// Cache TTL settings
    #[serde(default)]
    pub ttl: CacheTtl,

    /// Whether cache is enabled (default: true)
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MetaClientConfig {
    pub capacity: CacheCapacity,
    pub ttl: CacheTtl,
    pub options: MetaClientOptions,
}

impl MetaClientConfig {
    pub fn effective_ttl(&self) -> CacheTtl {
        if self.ttl.is_zero() {
            CacheTtl::for_sqlite()
        } else {
            self.ttl.clone()
        }
    }
}

/// Meta client behaviour configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientOptions {
    /// If true, the meta client runs in read-only mode.
    #[serde(default)]
    pub read_only: bool,
    /// Disable background maintenance tasks such as cache watchers.
    #[serde(default)]
    pub no_background_jobs: bool,
    /// Enable case-insensitive path resolution.
    #[serde(default)]
    pub case_insensitive: bool,
    /// Optional override for the session heartbeat interval.
    #[serde(default, with = "duration_option_serde")]
    pub session_heartbeat: Option<Duration>,
    /// Maximum symlink follow depth.
    #[serde(default = "default_max_symlinks")]
    pub max_symlinks: usize,
}

fn default_max_symlinks() -> usize {
    40
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            read_only: false,
            no_background_jobs: false,
            case_insensitive: false,
            session_heartbeat: None,
            max_symlinks: default_max_symlinks(),
        }
    }
}

/// Cache capacity configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheCapacity {
    /// Inode metadata cache capacity (includes attr, children, parent)
    #[serde(default = "default_inode_capacity")]
    pub inode: usize,

    /// Path resolution cache capacity
    #[serde(default = "default_path_capacity")]
    pub path: usize,
}

/// Cache TTL configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheTtl {
    /// Inode metadata cache TTL (includes attr, children, parent)
    #[serde(default, with = "duration_serde")]
    pub inode_ttl: Duration,

    /// Path resolution cache TTL
    #[serde(default, with = "duration_serde")]
    pub path_ttl: Duration,
}

// Default value functions
fn default_cache_enabled() -> bool {
    true
}

fn default_inode_capacity() -> usize {
    10000
}

fn default_path_capacity() -> usize {
    5000
}

impl Default for CacheCapacity {
    fn default() -> Self {
        Self {
            inode: default_inode_capacity(),
            path: default_path_capacity(),
        }
    }
}

impl CacheTtl {
    /// Get default TTL based on database backend type
    pub fn for_backend(backend: &str) -> Self {
        match backend {
            "sqlite" => Self::for_sqlite(),
            "postgres" => Self::for_postgres(),
            "etcd" => Self::for_etcd(),
            "redis" => Self::for_redis(),
            _ => Self::for_sqlite(),
        }
    }

    /// SQLite backend defaults (10s TTL for local database)
    pub fn for_sqlite() -> Self {
        Self {
            inode_ttl: Duration::from_secs(10),
            path_ttl: Duration::from_secs(10),
        }
    }

    /// PostgreSQL backend defaults (500ms TTL for network latency)
    pub fn for_postgres() -> Self {
        Self {
            inode_ttl: Duration::from_millis(500),
            path_ttl: Duration::from_millis(500),
        }
    }

    /// Etcd backend defaults (longer TTL improves slice cache reuse)
    pub fn for_etcd() -> Self {
        Self {
            inode_ttl: Duration::from_secs(2),
            path_ttl: Duration::from_secs(2),
        }
    }

    /// Redis backend defaults (similar to PostgreSQL latency expectations)
    pub fn for_redis() -> Self {
        Self {
            inode_ttl: Duration::from_millis(500),
            path_ttl: Duration::from_millis(500),
        }
    }

    /// Check if this is a zero/default TTL config
    pub fn is_zero(&self) -> bool {
        self.inode_ttl.is_zero() && self.path_ttl.is_zero()
    }
}

impl Default for CacheTtl {
    fn default() -> Self {
        // Return zero duration, will be replaced by backend-specific defaults
        Self {
            inode_ttl: Duration::ZERO,
            path_ttl: Duration::ZERO,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            capacity: CacheCapacity::default(),
            ttl: CacheTtl::default(),
            enabled: true,
        }
    }
}

impl CacheConfig {
    /// Validate cache configuration
    #[allow(dead_code)]
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled {
            if self.capacity.inode == 0 {
                return Err("inode cache capacity must be > 0".into());
            }
            if self.capacity.path == 0 {
                return Err("path cache capacity must be > 0".into());
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

/// Optional duration helper for serde (seconds as float/int)
mod duration_option_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(value: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(duration) => serializer.serialize_some(&duration.as_secs_f64()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<f64>::deserialize(deserializer)?;
        Ok(opt.map(Duration::from_secs_f64))
    }
}

// Added missing `backend_type` method to `DatabaseType` enum
impl DatabaseType {
    pub fn backend_type(&self) -> &str {
        match self {
            DatabaseType::Sqlite { .. } => "sqlite",
            DatabaseType::Postgres { .. } => "postgres",
            DatabaseType::Etcd { .. } => "etcd",
            DatabaseType::Redis { .. } => "redis",
        }
    }
}
