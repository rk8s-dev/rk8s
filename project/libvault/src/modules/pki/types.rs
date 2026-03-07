use crate::modules::pki::CertExt;
use builder_pattern::Builder;
use rustls::pki_types::CertificateDer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::utils::{deserialize_duration, serialize_duration};

/// Certificate key type discriminator used in unified route handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CertKeyType {
    Tls,
    Ssh,
    Pgp,
}

/// Request body for `POST /v1/pki/config/ca`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigCaRequest {
    pub pem_bundle: String,
}

/// Request body for `POST /v1/pki/root/generate/*`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RootGenerateRequest {
    pub common_name: Option<String>,
    pub alt_names: Option<String>,
    pub ttl: Option<String>,
    pub not_before_duration: Option<u64>,
    pub not_after: Option<String>,
    pub ou: Option<String>,
    pub organization: Option<String>,
    pub country: Option<String>,
    pub locality: Option<String>,
    pub province: Option<String>,
    pub street_address: Option<String>,
    pub postal_code: Option<String>,
    pub serial_number: Option<String>,
    pub exported: Option<String>,
    pub key_type: Option<String>,
    pub key_bits: Option<u32>,
    pub signature_bits: Option<u32>,
    pub use_pss: Option<bool>,
    pub permitted_dns_domains: Option<String>,
    pub max_path_length: Option<i32>,
}

/// Common payload returned by certificate issuing endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueCertificateResponse {
    pub certificate: String,
    pub private_key: String,
    pub private_key_type: String,
    pub serial_number: String,
    pub issuing_ca: String,
    #[serde(default)]
    pub ca_chain: String,
    pub expiration: i64,
}

impl IssueCertificateResponse {
    pub fn to_certs(&self) -> anyhow::Result<Vec<CertificateDer<'static>>> {
        let mut all_cert = self.certificate.clone();
        if self.ca_chain.trim().is_empty() {
            all_cert.push_str(&self.issuing_ca);
        } else {
            all_cert.push_str(&self.ca_chain);
        }

        all_cert.to_certs()
    }
}

/// Request body for `POST /v1/pki/issue/<role>`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, Builder)]
pub struct IssueCertificateRequest {
    pub common_name: Option<String>,
    pub alt_names: Option<String>,
    pub ip_sans: Option<String>,
    pub ttl: Option<String>,
}

/// Response body for `GET /v1/pki/cert/<serial>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchCertificateResponse {
    pub certificate: String,
    pub serial_number: String,
    pub ca_chain: String,
}

/// Request body for `POST /v1/pki/revoke`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeCertificateRequest {
    pub serial_number: String,
}

/// Request body for `POST /v1/pki/keys/generate/<type>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyGenerateRequest {
    pub key_name: String,
    #[serde(default)]
    pub key_bits: Option<u32>,
    #[serde(default)]
    pub key_type: Option<String>,
}

/// Response body for key generation/import operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyOperationResponse {
    pub key_id: String,
    pub key_name: String,
    pub key_type: String,
    pub key_bits: u32,
    #[serde(default)]
    pub private_key: Option<String>,
    #[serde(default)]
    pub iv: Option<String>,
}

/// Request body for `POST /v1/pki/keys/import`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyImportRequest {
    pub key_name: String,
    #[serde(default)]
    pub key_type: Option<String>,
    #[serde(default)]
    pub pem_bundle: Option<String>,
    #[serde(default)]
    pub hex_bundle: Option<String>,
    #[serde(default)]
    pub iv: Option<String>,
}

/// Request body for signing operations (`/keys/sign`, `/keys/verify`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeySignRequest {
    pub key_name: String,
    pub data: String,
}

/// Request body for `/keys/verify`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVerifyRequest {
    pub key_name: String,
    pub data: String,
    pub signature: String,
}

/// Request body for `/keys/encrypt` and `/keys/decrypt`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyCryptRequest {
    pub key_name: String,
    pub data: String,
    #[serde(default)]
    pub aad: Option<String>,
}

/// Response body for `/keys/sign`, `/keys/encrypt`, `/keys/decrypt` that return hex strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyHexResult {
    pub result: String,
}

/// Response body for `/keys/verify`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVerifyResult {
    pub result: bool,
}

/// Response body for `/v1/pki/ca` or `/v1/pki/ca/pem`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchCaResponse {
    pub certificate: String,
    #[serde(default)]
    pub ca_chain: Option<String>,
    #[serde(default)]
    pub issuing_ca: Option<String>,
    #[serde(default)]
    pub serial_number: Option<String>,
}

// ── SSH types ──

/// Request body for `POST /v1/pki/ssh/config/ca`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SshConfigCaRequest {
    #[serde(default)]
    pub key_type: Option<String>,
    #[serde(default)]
    pub key_bits: Option<u32>,
    #[serde(default)]
    pub private_key: Option<String>,
    #[serde(default)]
    pub public_key: Option<String>,
}

/// Response body for SSH CA public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfigCaResponse {
    pub public_key: String,
}

/// SSH role configuration stored in storage.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SshRoleEntry {
    #[serde(default = "default_ssh_cert_type")]
    pub cert_type: String,
    #[serde(default = "default_ssh_key_type")]
    pub key_type: String,
    #[serde(default = "default_ssh_key_bits")]
    pub key_bits: u32,
    #[serde(
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration",
        default = "default_ssh_ttl"
    )]
    pub ttl: Duration,
    #[serde(default)]
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub allowed_extensions: Vec<String>,
    #[serde(default)]
    pub default_extensions: HashMap<String, String>,
}

fn default_ssh_cert_type() -> String {
    "user".to_string()
}
fn default_ssh_key_type() -> String {
    "rsa".to_string()
}
fn default_ssh_key_bits() -> u32 {
    2048
}
fn default_ssh_ttl() -> Duration {
    Duration::from_secs(3600)
}

/// Request body for `POST /v1/pki/ssh/issue/{role}`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SshIssueCertificateRequest {
    pub key_id: String,
    #[serde(default)]
    pub valid_principals: Vec<String>,
    #[serde(default)]
    pub ttl: Option<String>,
    #[serde(default)]
    pub extensions: Option<HashMap<String, String>>,
}

/// Response body for SSH certificate issuance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshIssueCertificateResponse {
    pub signed_key: String,
    pub private_key: String,
    pub public_key: String,
    pub serial_number: String,
    pub expiration: i64,
}

/// Request body for `POST /v1/pki/ssh/sign/{role}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshSignKeyRequest {
    pub public_key: String,
    pub key_id: String,
    #[serde(default)]
    pub valid_principals: Vec<String>,
    #[serde(default)]
    pub ttl: Option<String>,
    #[serde(default)]
    pub extensions: Option<HashMap<String, String>>,
}

/// Response body for SSH public key signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshSignKeyResponse {
    pub signed_key: String,
    pub serial_number: String,
    pub expiration: i64,
}

// ── PGP types ──

/// Request body for `POST /v1/pki/pgp/generate/(exported|internal)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgpGenerateRequest {
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub key_type: Option<String>,
    #[serde(default)]
    pub key_bits: Option<u32>,
    #[serde(default)]
    pub ttl: Option<String>,
    pub key_name: String,
}

/// Response body for PGP key generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgpGenerateResponse {
    pub public_key: String,
    #[serde(default)]
    pub private_key: Option<String>,
    pub fingerprint: String,
    pub key_id: String,
}

/// Internal storage bundle for PGP keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgpKeyBundle {
    pub key_name: String,
    pub name: String,
    pub email: String,
    pub armored_secret_key: String,
    pub armored_public_key: String,
    pub fingerprint: String,
    pub key_id: String,
}

/// Request body for `POST /v1/pki/pgp/sign`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgpSignRequest {
    pub key_name: String,
    pub data: String,
}

/// Response body for PGP signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgpSignResponse {
    pub signature: String,
}

/// Request body for `POST /v1/pki/pgp/verify`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgpVerifyRequest {
    pub key_name: String,
    pub data: String,
    pub signature: String,
}

/// Response body for PGP verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgpVerifyResult {
    pub valid: bool,
}
