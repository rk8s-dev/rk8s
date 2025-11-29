//! Configuration error types.

use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Result type for configuration operations.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Errors that can occur during configuration loading and parsing.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Failed to read the configuration file.
    #[error("failed to read config file `{path}`: {source}")]
    FileRead { path: PathBuf, source: io::Error },

    /// Failed to parse YAML configuration.
    #[error("failed to parse YAML config: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    /// Failed to parse TOML configuration.
    #[error("failed to parse TOML config: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// Failed to parse JSON configuration.
    #[error("failed to parse JSON config: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Unable to determine file format from extension.
    #[error("unable to determine config format for file `{0}`. Expected .yaml, .yml, .toml, or .json")]
    UnknownFormat(PathBuf),

    /// Environment variable parsing error.
    #[error("failed to parse env var `{name}`: {message}")]
    EnvParse { name: String, message: String },

    /// Required field is missing.
    #[error("required configuration field `{0}` is missing")]
    MissingField(String),

    /// Configuration validation failed.
    #[error("configuration validation failed: {0}")]
    Validation(String),

    /// Multiple validation errors.
    #[error("configuration validation failed with {0} errors:\n{1}")]
    ValidationErrors(usize, String),

    /// Failed to determine user configuration directory.
    #[error("failed to determine user config directory")]
    NoConfigDir,

    /// Failed to create configuration directory.
    #[error("failed to create config directory `{path}`: {source}")]
    CreateDir { path: PathBuf, source: io::Error },

    /// Failed to write configuration file.
    #[error("failed to write config file `{path}`: {source}")]
    FileWrite { path: PathBuf, source: io::Error },

    /// Serialization error for YAML.
    #[error("failed to serialize config to YAML: {0}")]
    YamlSerialize(#[source] serde_yaml::Error),

    /// Serialization error for TOML.
    #[error("failed to serialize config to TOML: {0}")]
    TomlSerialize(#[source] toml::ser::Error),

    /// Serialization error for JSON.
    #[error("failed to serialize config to JSON: {0}")]
    JsonSerialize(#[source] serde_json::Error),
}
