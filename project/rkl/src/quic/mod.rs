pub mod client;
pub mod verifier;

#[derive(Debug, Clone)]
pub struct TLSConnectionConfig {
    pub enable_tls: bool,
    pub vault_url: String,
    pub bootstrap_token: String,
}