use std::{collections::HashMap, fmt};

use better_default::Default;
use openssl::ssl::SslVersion;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};
use serde_json::Value;

/// Configuration options for RustyVault.
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

/// Listener configuration.
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

/// Storage backend configuration.
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

    impl<'de> Visitor<'de> for TlsVersionVisitor {
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
    let listener: HashMap<String, Listener> = Deserialize::deserialize(deserializer)?;

    if listener.is_empty() {
        return Err(serde::de::Error::custom("Listener cannot be empty"));
    }

    Ok(listener)
}
