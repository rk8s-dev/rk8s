use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshKeyGenerateRequest {
    pub key_name: String,
    #[serde(default)]
    pub key_type: String, // "rsa" or "ed25519"
    #[serde(default)]
    pub passphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshCertSignRequest {
    pub ca_name: String,
    pub public_key: String,
    pub key_name: String,
    pub valid_principals: String,
    pub valid_after: String,
    pub valid_before: String,
    pub cert_type: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub passphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshRole {
    pub name: String,
    #[serde(default = "default_key_type")]
    pub key_type: String, // "user" or "host"
    #[serde(default = "default_ttl")]
    pub ttl: String,
    #[serde(default = "default_max_ttl")]
    pub max_ttl: String,
    #[serde(default = "default_allowed_users")]
    pub allowed_users: String, // comma separated or "*"
    #[serde(default)]
    pub allow_user_certificates: bool,
    #[serde(default)]
    pub allow_host_certificates: bool,
}

fn default_key_type() -> String {
    "user".to_string()
}
fn default_ttl() -> String {
    "24h".to_string()
}
fn default_max_ttl() -> String {
    "72h".to_string()
}
fn default_allowed_users() -> String {
    "*".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokedKey {
    pub serial: u64,
    pub key_id: String,
    pub revoked_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaGenerateRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub key_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyGenerateRequest {
    pub name: String,
    #[serde(default)]
    pub key_type: String,
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignRequest {
    pub ca_name: String,
    pub key_name: String,
    #[serde(default)]
    pub principals: Vec<String>,
    #[serde(default)]
    pub ttl: String,
    #[serde(default)]
    pub cert_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshCertResponse {
    pub certificate: String,
    pub public_key: String,
    #[serde(default)]
    pub private_key: Option<String>,
    pub expiration: i64,
    #[serde(default)]
    pub fingerprint: Option<String>,
}
