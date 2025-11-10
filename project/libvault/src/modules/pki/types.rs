use crate::modules::pki::CertExt;
use builder_pattern::Builder;
use rustls::pki_types::CertificateDer;
use serde::{Deserialize, Serialize};

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
