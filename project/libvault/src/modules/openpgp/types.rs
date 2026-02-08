use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgpGenerateRequest {
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub passphrase: String,
    #[serde(default)]
    pub key_bits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgpKeyResponse {
    pub name: String,
    pub public_key: String,
    pub secret_key: String,
    #[serde(default)]
    pub created_at: String,
}
