//! This module defines and handles the config file options for RustyVault application.
//! For instance, the IP address and port for the RustyVault to listen on is handled in this
//! module.

use std::{collections::HashMap, fmt, fs, path::Path};

use better_default::Default;
use openssl::ssl::SslVersion;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};
use serde_json::Value;

use crate::errors::RvError;

/// A struct that contains several configurable options of RustyVault server
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(deserialize_with = "validate_listener")]
    pub listener: HashMap<String, Listener>,
    #[serde(deserialize_with = "validate_storage")]
    pub storage: HashMap<String, Storage>,
    #[serde(default)]
    pub api_addr: String,
    #[serde(default)]
    pub log_format: String,
    #[serde(default)]
    pub log_level: String,
    #[serde(default)]
    pub pid_file: String,
    #[serde(default)]
    pub work_dir: String,
    #[serde(default, deserialize_with = "parse_bool_string")]
    pub daemon: bool,
    #[serde(default)]
    pub daemon_user: String,
    #[serde(default)]
    pub daemon_group: String,
    #[serde(default = "default_collection_interval")]
    pub collection_interval: u64,
    #[serde(default = "default_hmac_level")]
    pub mount_entry_hmac_level: MountEntryHMACLevel,
    #[serde(default = "default_mounts_monitor_interval")]
    #[default(5)]
    pub mounts_monitor_interval: u64,
}

#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MountEntryHMACLevel {
    #[default]
    None,
    Compat,
    High,
}

fn default_hmac_level() -> MountEntryHMACLevel {
    MountEntryHMACLevel::None
}

fn default_collection_interval() -> u64 {
    15
}

fn default_mounts_monitor_interval() -> u64 {
    5
}

/// A struct that contains several configurable options for networking stuffs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listener {
    #[serde(default)]
    pub ltype: String,
    pub address: String,
    #[serde(default = "default_bool_true", deserialize_with = "parse_bool_string")]
    pub tls_disable: bool,
    #[serde(default)]
    pub tls_cert_file: String,
    #[serde(default)]
    pub tls_key_file: String,
    #[serde(default)]
    pub tls_client_ca_file: String,
    #[serde(default, deserialize_with = "parse_bool_string")]
    pub tls_disable_client_certs: bool,
    #[serde(default, deserialize_with = "parse_bool_string")]
    pub tls_require_and_verify_client_cert: bool,
    #[serde(
        default = "default_tls_min_version",
        serialize_with = "serialize_tls_version",
        deserialize_with = "deserialize_tls_version"
    )]
    pub tls_min_version: SslVersion,
    #[serde(
        default = "default_tls_max_version",
        serialize_with = "serialize_tls_version",
        deserialize_with = "deserialize_tls_version"
    )]
    pub tls_max_version: SslVersion,
    #[serde(default = "default_tls_cipher_suites")]
    pub tls_cipher_suites: String,
}

/// A struct that contains several configurable options for storage stuffs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Storage {
    #[serde(default)]
    pub stype: String,
    #[serde(flatten)]
    pub config: HashMap<String, Value>,
}

static STORAGE_TYPE_KEYWORDS: &[&str] = &["file", "mysql", "xline"];

fn default_bool_true() -> bool {
    true
}

fn parse_bool_string<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;
    match value {
        Value::Bool(b) => Ok(b),
        Value::String(s) => match s.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(serde::de::Error::custom("Invalid value for bool")),
        },
        _ => Err(serde::de::Error::custom("Invalid value for bool")),
    }
}

fn default_tls_min_version() -> SslVersion {
    SslVersion::TLS1_2
}

fn default_tls_max_version() -> SslVersion {
    SslVersion::TLS1_3
}

fn default_tls_cipher_suites() -> String {
    "HIGH:!PSK:!SRP:!3DES".to_string()
}

fn serialize_tls_version<S>(version: &SslVersion, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match *version {
        SslVersion::TLS1 => serializer.serialize_str("tls10"),
        SslVersion::TLS1_1 => serializer.serialize_str("tls11"),
        SslVersion::TLS1_2 => serializer.serialize_str("tls12"),
        SslVersion::TLS1_3 => serializer.serialize_str("tls13"),
        _ => unreachable!("unexpected SSL/TLS version: {:?}", version),
    }
}

fn deserialize_tls_version<'de, D>(deserializer: D) -> Result<SslVersion, D::Error>
where
    D: Deserializer<'de>,
{
    struct TlsVersionVisitor;

    impl Visitor<'_> for TlsVersionVisitor {
        type Value = SslVersion;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string representing an SSL version")
        }

        fn visit_str<E>(self, value: &str) -> Result<SslVersion, E>
        where
            E: de::Error,
        {
            match value {
                "tls10" => Ok(SslVersion::TLS1),
                "tls11" => Ok(SslVersion::TLS1_1),
                "tls12" => Ok(SslVersion::TLS1_2),
                "tls13" => Ok(SslVersion::TLS1_3),
                _ => Err(E::custom(format!("unexpected SSL/TLS version: {value}"))),
            }
        }
    }

    deserializer.deserialize_str(TlsVersionVisitor)
}

fn validate_storage<'de, D>(deserializer: D) -> Result<HashMap<String, Storage>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let storage: HashMap<String, Storage> = Deserialize::deserialize(deserializer)?;

    for key in storage.keys() {
        if !STORAGE_TYPE_KEYWORDS.contains(&key.as_str()) {
            return Err(serde::de::Error::custom("Invalid storage key"));
        }
    }

    Ok(storage)
}

fn validate_listener<'de, D>(deserializer: D) -> Result<HashMap<String, Listener>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let listeners: HashMap<String, Listener> = Deserialize::deserialize(deserializer)?;

    for (key, listener) in &listeners {
        if key != "tcp" {
            return Err(serde::de::Error::custom("Invalid listener key"));
        }

        if !listener.tls_disable
            && (listener.tls_cert_file.is_empty() || listener.tls_key_file.is_empty())
        {
            return Err(serde::de::Error::custom(
                "when tls_disable is false, tls_cert_file and tls_key_file must be configured",
            ));
        }

        if !listener.tls_disable
            && listener.tls_require_and_verify_client_cert
            && listener.tls_disable_client_certs
        {
            return Err(serde::de::Error::custom(
                "'tls_disable_client_certs' and 'tls_require_and_verify_client_cert' are mutually exclusive",
            ));
        }
    }

    Ok(listeners)
}

impl Config {
    pub fn merge(&mut self, other: Config) {
        self.listener.extend(other.listener);
        self.storage.extend(other.storage);
        if !other.api_addr.is_empty() {
            self.api_addr = other.api_addr;
        }

        if !other.log_format.is_empty() {
            self.log_format = other.log_format;
        }

        if !other.log_level.is_empty() {
            self.log_level = other.log_level;
        }

        if !other.pid_file.is_empty() {
            self.pid_file = other.pid_file;
        }

        if other.mount_entry_hmac_level != MountEntryHMACLevel::None {
            self.mount_entry_hmac_level = other.mount_entry_hmac_level;
        }
    }
}

pub fn load_config(path: &str) -> Result<Config, RvError> {
    let f = Path::new(path);
    if f.is_dir() {
        load_config_dir(path)
    } else if f.is_file() {
        load_config_file(path)
    } else {
        Err(RvError::ErrConfigPathInvalid)
    }
}

fn load_config_dir(dir: &str) -> Result<Config, RvError> {
    log::debug!("load_config_dir: {dir}");
    let mut paths: Vec<String> = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            if let Some(ext) = path.extension()
                && (ext == "hcl" || ext == "json")
            {
                let filename = path.to_string_lossy().into_owned();
                paths.push(filename);
            }
        }
    }

    let mut result = None;

    for path in paths {
        log::debug!("load_config_dir path: {path}");
        let config = load_config_file(&path)?;
        if result.is_none() {
            result = Some(config.clone());
        } else {
            result.as_mut().unwrap().merge(config);
        }
    }

    result.ok_or(RvError::ErrConfigLoadFailed)
}

fn load_config_file(path: &str) -> Result<Config, RvError> {
    log::debug!("load_config_file: {path}");
    let file = fs::File::open(path)?;

    if path.ends_with(".hcl") {
        let mut config: Config = hcl::from_reader(file)?;
        set_config_type_field(&mut config)?;
        check_config(&config)?;
        Ok(config)
    } else if path.ends_with(".json") {
        let mut config: Config = serde_json::from_reader(file)?;
        set_config_type_field(&mut config)?;
        check_config(&config)?;
        Ok(config)
    } else {
        Err(RvError::ErrConfigPathInvalid)
    }
}

fn set_config_type_field(config: &mut Config) -> Result<(), RvError> {
    config
        .storage
        .iter_mut()
        .for_each(|(key, value)| value.stype.clone_from(key));
    config
        .listener
        .iter_mut()
        .for_each(|(key, value)| value.ltype.clone_from(key));
    Ok(())
}

fn check_config(config: &Config) -> Result<(), RvError> {
    if config.storage.len() != 1 {
        return Err(RvError::ErrConfigStorageNotFound);
    }

    if config.listener.len() != 1 {
        return Err(RvError::ErrConfigListenerNotFound);
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use std::{env, fs, io::prelude::*};

    use super::*;
    use crate::test_utils::TEST_DIR;

    fn write_file(path: &str, config: &str) -> Result<(), RvError> {
        let mut file = fs::File::create(path)?;

        file.write_all(config.as_bytes())?;

        file.flush()?;

        Ok(())
    }

    #[test]
    fn test_load_config() {
        let dir = env::temp_dir().join(*TEST_DIR).join("test_load_config");
        assert!(fs::create_dir(&dir).is_ok());

        let file_path = dir.join("config.hcl");
        let path = file_path.to_str().unwrap_or("config.hcl");

        let hcl_config_str = r#"
            storage "file" {
              path    = "./vault/data"
            }

            listener "tcp" {
              address     = "127.0.0.1:8200"
            }

            api_addr = "http://127.0.0.1:8200"
            log_level = "debug"
            log_format = "{date} {req.path}"
            pid_file = "/tmp/rusty_vault.pid"
        "#;

        assert!(write_file(path, hcl_config_str).is_ok());

        let config = load_config(path);
        assert!(config.is_ok());
        let hcl_config = config.unwrap();
        println!("hcl config: {:?}", hcl_config);

        let json_config_str = r#"{
            "storage": {
                "file": {
                    "path": "./vault/data"
                }
            },
            "listener": {
                "tcp": {
                    "address": "127.0.0.1:8200"
                }
            },
            "api_addr": "http://127.0.0.1:8200",
            "log_level": "debug",
            "log_format": "{date} {req.path}",
            "pid_file": "/tmp/rusty_vault.pid"
        }"#;

        let file_path = dir.join("config.json");
        let path = file_path.to_str().unwrap_or("config.json");
        assert!(write_file(path, json_config_str).is_ok());

        let config = load_config(path);
        assert!(config.is_ok());
        let json_config = config.unwrap();
        println!("json config: {:?}", json_config);

        let hcl_config_value = serde_json::to_value(&hcl_config);
        assert!(hcl_config_value.is_ok());
        let hcl_config_value: Value = hcl_config_value.unwrap();

        let json_config_value = serde_json::to_value(&json_config);
        assert!(json_config_value.is_ok());
        let json_config_value: Value = json_config_value.unwrap();
        assert_eq!(hcl_config_value, json_config_value);

        assert_eq!(json_config.listener.len(), 1);
        assert_eq!(json_config.storage.len(), 1);
        assert_eq!(json_config.api_addr.as_str(), "http://127.0.0.1:8200");
        assert_eq!(json_config.log_format.as_str(), "{date} {req.path}");
        assert_eq!(json_config.log_level.as_str(), "debug");
        assert_eq!(json_config.pid_file.as_str(), "/tmp/rusty_vault.pid");
        assert_eq!(json_config.work_dir.as_str(), "");
        assert_eq!(json_config.daemon, false);
        assert_eq!(json_config.daemon_user.as_str(), "");
        assert_eq!(json_config.daemon_group.as_str(), "");
        assert_eq!(
            json_config.mount_entry_hmac_level,
            MountEntryHMACLevel::None
        );

        let (_, listener) = json_config.listener.iter().next().unwrap();
        assert!(listener.tls_disable);
        assert_eq!(listener.ltype.as_str(), "tcp");
        assert_eq!(listener.address.as_str(), "127.0.0.1:8200");

        let (_, storage) = json_config.storage.iter().next().unwrap();
        assert_eq!(storage.stype.as_str(), "file");
        assert_eq!(storage.config.len(), 1);
        let (_, path) = storage.config.iter().next().unwrap();
        assert_eq!(path.as_str(), Some("./vault/data"));
    }

    #[test]
    fn test_load_config_dir() {
        let dir = env::temp_dir().join(*TEST_DIR).join("test_load_config_dir");
        assert!(fs::create_dir(&dir).is_ok());

        let file_path = dir.join("config1.hcl");
        let path = file_path.to_str().unwrap_or("config1.hcl");

        let hcl_config_str = r#"
            storage "file" {
              path    = "./vault/data"
            }

            listener "tcp" {
              address     = "127.0.0.1:8200"
              tls_disable = "true"
            }

            api_addr = "http://127.0.0.1:8200"
            log_level = "debug"
            log_format = "{date} {req.path}"
            pid_file = "/tmp/rusty_vault.pid"
            mount_entry_hmac_level = "compat"
        "#;

        assert!(write_file(path, hcl_config_str).is_ok());

        let file_path = dir.join("config2.hcl");
        let path = file_path.to_str().unwrap_or("config2.hcl");

        let hcl_config_str = r#"
            storage "file" {
              address    = "127.0.0.1:8899"
            }

            listener "tcp" {
              address     = "127.0.0.1:8800"
              tls_disable = true
            }

            log_level = "info"
        "#;

        assert!(write_file(path, hcl_config_str).is_ok());

        let config = load_config(dir.to_str().unwrap());
        println!("config: {:?}", config);
        assert!(config.is_ok());
        let hcl_config = config.unwrap();
        println!("hcl config: {:?}", hcl_config);
        assert_eq!(
            hcl_config.mount_entry_hmac_level,
            MountEntryHMACLevel::Compat
        );

        let (_, listener) = hcl_config.listener.iter().next().unwrap();
        assert!(listener.tls_disable);
    }

    #[test]
    fn test_load_config_tls() {
        let dir = env::temp_dir().join(*TEST_DIR).join("test_load_config_tls");
        assert!(fs::create_dir(&dir).is_ok());

        let file_path = dir.join("config.hcl");
        let path = file_path.to_str().unwrap_or("config.hcl");

        let hcl_config_str = r#"
            storage "file" {
              path    = "./vault/data"
            }

            listener "tcp" {
              address     = "127.0.0.1:8200"
              tls_disable = false
              tls_cert_file = "./cert/test.crt"
              tls_key_file = "./cert/test.key"
              tls_client_ca_file = "./cert/ca.pem"
              tls_min_version = "tls12"
              tls_max_version = "tls13"
            }

            api_addr = "http://127.0.0.1:8200"
            log_level = "debug"
            log_format = "{date} {req.path}"
            pid_file = "/tmp/rusty_vault.pid"
            mount_entry_hmac_level = "high"
        "#;

        assert!(write_file(path, hcl_config_str).is_ok());

        let config = load_config(path);
        assert!(config.is_ok());
        let hcl_config = config.unwrap();
        println!("hcl config: {:?}", hcl_config);

        assert_eq!(hcl_config.listener.len(), 1);
        assert_eq!(hcl_config.storage.len(), 1);
        assert_eq!(hcl_config.api_addr.as_str(), "http://127.0.0.1:8200");
        assert_eq!(hcl_config.log_format.as_str(), "{date} {req.path}");
        assert_eq!(hcl_config.log_level.as_str(), "debug");
        assert_eq!(hcl_config.pid_file.as_str(), "/tmp/rusty_vault.pid");
        assert_eq!(hcl_config.work_dir.as_str(), "");
        assert_eq!(hcl_config.daemon, false);
        assert_eq!(hcl_config.daemon_user.as_str(), "");
        assert_eq!(hcl_config.daemon_group.as_str(), "");
        assert_eq!(hcl_config.mount_entry_hmac_level, MountEntryHMACLevel::High);

        let (_, listener) = hcl_config.listener.iter().next().unwrap();
        assert_eq!(listener.ltype.as_str(), "tcp");
        assert_eq!(listener.address.as_str(), "127.0.0.1:8200");
        assert_eq!(listener.tls_disable, false);
        assert_eq!(listener.tls_cert_file.as_str(), "./cert/test.crt");
        assert_eq!(listener.tls_key_file.as_str(), "./cert/test.key");
        assert_eq!(listener.tls_client_ca_file.as_str(), "./cert/ca.pem");
        assert_eq!(listener.tls_disable_client_certs, false);
        assert_eq!(listener.tls_require_and_verify_client_cert, false);
        assert_eq!(listener.tls_min_version, SslVersion::TLS1_2);
        assert_eq!(listener.tls_max_version, SslVersion::TLS1_3);

        let (_, storage) = hcl_config.storage.iter().next().unwrap();
        assert_eq!(storage.stype.as_str(), "file");
        assert_eq!(storage.config.len(), 1);
        let (_, path) = storage.config.iter().next().unwrap();
        assert_eq!(path.as_str(), Some("./vault/data"));
    }
}
