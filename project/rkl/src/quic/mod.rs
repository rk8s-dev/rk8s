use crate::commands::pod::TLSConnectionArgs;

pub mod client;
pub mod verifier;

#[derive(Debug, Clone)]
pub struct TLSConnectionConfig {
    pub enable_tls: bool,
    pub vault_url: String,
    pub bootstrap_token: String,
}

impl From<TLSConnectionArgs> for TLSConnectionConfig {
    fn from(value: TLSConnectionArgs) -> Self {
        Self {
            enable_tls: value.enable_tls,
            vault_url: value.vault_url.unwrap_or_default(),
            bootstrap_token: value.bootstrap_token.unwrap_or_default(),
        }
    }
}
