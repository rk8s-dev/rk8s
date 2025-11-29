//! Configuration loading utilities.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::error::{ConfigError, Result};

/// Supported configuration file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// YAML format (.yaml, .yml)
    Yaml,
    /// TOML format (.toml)
    Toml,
    /// JSON format (.json)
    Json,
}

impl FileFormat {
    /// Detect format from file extension.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        match path.extension().and_then(|e| e.to_str()) {
            Some("yaml" | "yml") => Ok(FileFormat::Yaml),
            Some("toml") => Ok(FileFormat::Toml),
            Some("json") => Ok(FileFormat::Json),
            _ => Err(ConfigError::UnknownFormat(path.to_path_buf())),
        }
    }

    /// Parse configuration content in this format.
    pub fn parse<T: DeserializeOwned>(&self, content: &str) -> Result<T> {
        match self {
            FileFormat::Yaml => serde_yaml::from_str(content).map_err(ConfigError::from),
            FileFormat::Toml => toml::from_str(content).map_err(ConfigError::from),
            FileFormat::Json => serde_json::from_str(content).map_err(ConfigError::from),
        }
    }

    /// Serialize configuration to this format.
    pub fn serialize<T: Serialize>(&self, config: &T) -> Result<String> {
        match self {
            FileFormat::Yaml => serde_yaml::to_string(config).map_err(ConfigError::YamlSerialize),
            FileFormat::Toml => toml::to_string_pretty(config).map_err(ConfigError::TomlSerialize),
            FileFormat::Json => {
                serde_json::to_string_pretty(config).map_err(ConfigError::JsonParse)
            }
        }
    }
}

/// Configuration loader with builder pattern.
///
/// Supports loading configuration from files with optional environment variable overrides.
///
/// # Example
///
/// ```ignore
/// use common::config::ConfigLoader;
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// struct AppConfig {
///     host: String,
///     port: u16,
/// }
///
/// let config: AppConfig = ConfigLoader::from_file("config.yaml")?
///     .with_env_prefix("APP")
///     .build()?;
/// ```
pub struct ConfigLoader {
    content: String,
    format: FileFormat,
    env_prefix: Option<String>,
}

impl ConfigLoader {
    /// Create a new config loader from file content.
    pub fn new(content: impl Into<String>, format: FileFormat) -> Self {
        Self {
            content: content.into(),
            format,
            env_prefix: None,
        }
    }

    /// Load configuration from a file path.
    ///
    /// The file format is detected automatically from the file extension.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let format = FileFormat::from_path(path)?;
        let content = fs::read_to_string(path).map_err(|e| ConfigError::FileRead {
            path: path.to_path_buf(),
            source: e,
        })?;
        Ok(Self::new(content, format))
    }

    /// Load configuration from a file path with explicit format.
    pub fn from_file_with_format(path: impl AsRef<Path>, format: FileFormat) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).map_err(|e| ConfigError::FileRead {
            path: path.to_path_buf(),
            source: e,
        })?;
        Ok(Self::new(content, format))
    }

    /// Set an environment variable prefix for overriding configuration values.
    ///
    /// When set, environment variables with the format `{PREFIX}_{FIELD_NAME}` will
    /// override the corresponding configuration values.
    ///
    /// # Example
    ///
    /// With prefix "APP", the environment variable `APP_PORT` would override the `port` field.
    pub fn with_env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = Some(prefix.into());
        self
    }

    /// Build the configuration.
    ///
    /// Parses the configuration content and returns the deserialized configuration.
    pub fn build<T: DeserializeOwned>(self) -> Result<T> {
        self.format.parse(&self.content)
    }

    /// Get the file format.
    pub fn format(&self) -> FileFormat {
        self.format
    }

    /// Get the raw content.
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// Get the user configuration directory for the application.
///
/// Returns `{config_dir}/{app_name}` where `config_dir` is platform-specific:
/// - Linux: `~/.config`
/// - macOS: `~/Library/Application Support`
/// - Windows: `{FOLDERID_RoamingAppData}`
pub fn user_config_dir(app_name: &str) -> Result<PathBuf> {
    let config_dir = dirs::config_dir().ok_or(ConfigError::NoConfigDir)?;
    Ok(config_dir.join(app_name))
}

/// Get the user configuration file path.
///
/// Returns `{config_dir}/{app_name}/{config_name}.toml` by default.
pub fn user_config_path(app_name: &str, config_name: Option<&str>) -> Result<PathBuf> {
    let dir = user_config_dir(app_name)?;
    let filename = config_name.unwrap_or(app_name);
    Ok(dir.join(format!("{filename}.toml")))
}

/// Load configuration from the user config directory.
///
/// Attempts to load from `{config_dir}/{app_name}/{config_name}.toml`.
/// If the file doesn't exist, returns the default configuration.
pub fn load_user_config<T>(app_name: &str, config_name: Option<&str>) -> Result<T>
where
    T: DeserializeOwned + Default,
{
    let path = user_config_path(app_name, config_name)?;
    if path.exists() {
        ConfigLoader::from_file(&path)?.build()
    } else {
        Ok(T::default())
    }
}

/// Save configuration to the user config directory.
///
/// Saves to `{config_dir}/{app_name}/{config_name}.toml`.
/// Creates the directory if it doesn't exist.
pub fn save_user_config<T>(app_name: &str, config_name: Option<&str>, config: &T) -> Result<()>
where
    T: Serialize,
{
    let path = user_config_path(app_name, config_name)?;

    // Create directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| ConfigError::CreateDir {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    let content = FileFormat::Toml.serialize(config)?;
    fs::write(&path, content).map_err(|e| ConfigError::FileWrite { path, source: e })
}

/// Parse an environment variable with type conversion.
///
/// # Example
///
/// ```ignore
/// let port: u16 = parse_env("PORT", Some(8080))?;
/// ```
pub fn parse_env<T>(name: &str, default: Option<T>) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(value) => value.parse().map_err(|e: T::Err| ConfigError::EnvParse {
            name: name.to_string(),
            message: e.to_string(),
        }),
        Err(_) => default.ok_or_else(|| ConfigError::MissingField(name.to_string())),
    }
}

/// Parse an optional environment variable with type conversion.
///
/// Returns `None` if the environment variable is not set.
pub fn parse_env_opt<T>(name: &str) -> Result<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(name) {
        Ok(value) => {
            let parsed = value.parse().map_err(|e: T::Err| ConfigError::EnvParse {
                name: name.to_string(),
                message: e.to_string(),
            })?;
            Ok(Some(parsed))
        }
        Err(_) => Ok(None),
    }
}

/// Configuration validation builder.
///
/// Collects validation errors and reports them together.
///
/// # Example
///
/// ```ignore
/// use common::config::ConfigValidator;
///
/// let mut validator = ConfigValidator::new();
/// validator.require("host", config.host.is_some());
/// validator.require_non_empty("name", &config.name);
/// validator.finish()?;
/// ```
#[derive(Debug, Default)]
pub struct ConfigValidator {
    errors: Vec<String>,
}

impl ConfigValidator {
    /// Create a new validator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a requirement check.
    pub fn require(&mut self, field: &str, condition: bool) -> &mut Self {
        if !condition {
            self.errors
                .push(format!("field `{field}` failed validation"));
        }
        self
    }

    /// Require a field to be non-empty.
    pub fn require_non_empty(&mut self, field: &str, value: &str) -> &mut Self {
        if value.is_empty() {
            self.errors.push(format!("field `{field}` cannot be empty"));
        }
        self
    }

    /// Require a field to be Some.
    pub fn require_some<T>(&mut self, field: &str, value: &Option<T>) -> &mut Self {
        if value.is_none() {
            self.errors
                .push(format!("field `{field}` is required but was not set"));
        }
        self
    }

    /// Add a custom error message.
    pub fn add_error(&mut self, message: impl Into<String>) -> &mut Self {
        self.errors.push(message.into());
        self
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get the error count.
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Finish validation and return an error if any checks failed.
    pub fn finish(self) -> Result<()> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            let count = self.errors.len();
            let messages = self.errors.join("\n  - ");
            Err(ConfigError::ValidationErrors(count, format!("  - {messages}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct TestConfig {
        host: String,
        port: u16,
    }

    #[test]
    fn test_yaml_parsing() {
        let yaml = r#"
host: "localhost"
port: 8080
"#;
        let loader = ConfigLoader::new(yaml, FileFormat::Yaml);
        let config: TestConfig = loader.build().unwrap();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_toml_parsing() {
        let toml = r#"
host = "localhost"
port = 8080
"#;
        let loader = ConfigLoader::new(toml, FileFormat::Toml);
        let config: TestConfig = loader.build().unwrap();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_json_parsing() {
        let json = r#"{"host": "localhost", "port": 8080}"#;
        let loader = ConfigLoader::new(json, FileFormat::Json);
        let config: TestConfig = loader.build().unwrap();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_format_detection() {
        assert_eq!(
            FileFormat::from_path("config.yaml").unwrap(),
            FileFormat::Yaml
        );
        assert_eq!(
            FileFormat::from_path("config.yml").unwrap(),
            FileFormat::Yaml
        );
        assert_eq!(
            FileFormat::from_path("config.toml").unwrap(),
            FileFormat::Toml
        );
        assert_eq!(
            FileFormat::from_path("config.json").unwrap(),
            FileFormat::Json
        );
    }

    #[test]
    fn test_unknown_format() {
        let result = FileFormat::from_path("config.txt");
        assert!(matches!(result, Err(ConfigError::UnknownFormat(_))));
    }

    #[test]
    fn test_validator_empty() {
        let validator = ConfigValidator::new();
        assert!(!validator.has_errors());
        validator.finish().unwrap();
    }

    #[test]
    fn test_validator_with_errors() {
        let mut validator = ConfigValidator::new();
        validator.require("field1", false);
        validator.require_non_empty("field2", "");
        validator.require_some::<String>("field3", &None);

        assert!(validator.has_errors());
        assert_eq!(validator.error_count(), 3);

        let result = validator.finish();
        assert!(result.is_err());
    }

    #[test]
    fn test_validator_passing() {
        let mut validator = ConfigValidator::new();
        validator.require("field1", true);
        validator.require_non_empty("field2", "value");
        validator.require_some("field3", &Some("value"));

        assert!(!validator.has_errors());
        validator.finish().unwrap();
    }

    #[test]
    fn test_parse_env_with_default() {
        // Clear any existing var
        // SAFETY: This is only called in tests where we control the environment
        unsafe {
            std::env::remove_var("TEST_CONFIG_VAR");
        }

        let result: u16 = parse_env("TEST_CONFIG_VAR", Some(8080)).unwrap();
        assert_eq!(result, 8080);
    }

    #[test]
    fn test_parse_env_without_default() {
        // SAFETY: This is only called in tests where we control the environment
        unsafe {
            std::env::remove_var("TEST_CONFIG_VAR_REQUIRED");
        }

        let result: Result<u16> = parse_env("TEST_CONFIG_VAR_REQUIRED", None);
        assert!(matches!(result, Err(ConfigError::MissingField(_))));
    }

    #[test]
    fn test_parse_env_with_value() {
        // SAFETY: This is only called in tests where we control the environment
        unsafe {
            std::env::set_var("TEST_CONFIG_PORT", "9090");
        }

        let result: u16 = parse_env("TEST_CONFIG_PORT", Some(8080)).unwrap();
        assert_eq!(result, 9090);

        // SAFETY: This is only called in tests where we control the environment
        unsafe {
            std::env::remove_var("TEST_CONFIG_PORT");
        }
    }

    #[test]
    fn test_parse_env_opt() {
        // SAFETY: This is only called in tests where we control the environment
        unsafe {
            std::env::remove_var("TEST_CONFIG_OPT_VAR");
        }

        let result: Option<u16> = parse_env_opt("TEST_CONFIG_OPT_VAR").unwrap();
        assert!(result.is_none());

        // SAFETY: This is only called in tests where we control the environment
        unsafe {
            std::env::set_var("TEST_CONFIG_OPT_VAR", "1234");
        }
        let result: Option<u16> = parse_env_opt("TEST_CONFIG_OPT_VAR").unwrap();
        assert_eq!(result, Some(1234));

        // SAFETY: This is only called in tests where we control the environment
        unsafe {
            std::env::remove_var("TEST_CONFIG_OPT_VAR");
        }
    }

    #[test]
    fn test_user_config_path() {
        let path = user_config_path("rk8s", Some("rkb")).unwrap();
        assert!(path.ends_with("rkb.toml"));
    }
}
