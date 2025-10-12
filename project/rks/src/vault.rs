use crate::protocol::config::config;
use anyhow::Context;
use derive_more::Deref;
use libvault::RustyVault;
use libvault::core::SealConfig;
use libvault::modules::ResponseExt;
use libvault::modules::pki::types::{IssueCertificateRequest, IssueCertificateResponse};
use libvault::storage::xline::XlineBackend;
use log::{debug, info};
use rand::RngCore;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Deref)]
pub struct Vault {
    #[deref]
    vault: RustyVault,
    root_token: String,
    join_token: String,
}

#[allow(dead_code)]
pub struct Persisted<T> {
    state: T,
    path: PathBuf,
}

impl Vault {
    pub fn new() -> anyhow::Result<Self> {
        let endpoints = &config().xline_config.endpoints;
        info!(
            "[vault] constructing backend with {} endpoint(s)",
            endpoints.len()
        );
        debug!("[vault] endpoints: {:?}", endpoints);
        let backend = XlineBackend::with_endpoints(endpoints);

        Ok(Self {
            vault: RustyVault::new(Arc::new(backend), None)?,
            root_token: String::new(),
            join_token: String::new(),
        })
    }

    pub fn join_token(&self) -> &str {
        self.join_token.as_str()
    }

    async fn write_policies(&self) -> anyhow::Result<()> {
        let rks_role = json!({
            "key_type": "ec",
            "key_bits": 256,
            "allow_ip_sans": true,
            "allowed_domains": "rks.svc.cluster.local",
            "allow_subdomains": true,
            "server_flag": true,
            "client_flag": true,
            "ttl": "30d",
            "max_ttl": "60d",
            "no_store": false,
            "generate_lease": false,
        });

        self.vault
            .write(
                Some(self.root_token.as_str()),
                "pki/roles/rks-node",
                rks_role.to_map()?,
            )
            .await
            .with_context(|| "Failed to write policy")?;
        info!("[vault] published role policy pki/roles/rks-node");

        let rks_role = json!({
            "key_type": "ec",
            "key_bits": 256,
            "allow_ip_sans": true,
            "allowed_domains": "rkl.svc.cluster.local",
            "allow_subdomains": true,
            "server_flag": true,
            "client_flag": true,
            "ttl": "15d",
            "max_ttl": "30d",
            "no_store": false,
            "generate_lease": false,
        });

        self.vault
            .write(
                Some(self.root_token.as_str()),
                "pki/roles/rkl-node",
                rks_role.to_map()?,
            )
            .await
            .with_context(|| "Failed to write policy")?;
        info!("[vault] published role policy pki/roles/rkl-node");
        Ok(())
    }

    async fn generate_root_ca(&self) -> anyhow::Result<()> {
        let payload = json!({
            "common_name": "rk8s root CA",
            "alt_names": "rk8s.github.io",
            "ttl": "87600h",
            "not_before_duration": 30,
            "organization": "rk8s organization",
            "country": "CN",
            "province": "ZJ",
            "locality": "HangZhou",
            "exported": "exported",
            "key_type": "ec",
            "key_bits": 256,
            "signature_bits": 384,
            "use_pss": false,
            "permitted_dns_domains": "rkl.svc.cluster.local,rks.scv.cluster.local",
            "max_path_length": 1,
        });

        let resp = self
            .vault
            .write(
                Some(self.root_token.as_str()),
                "pki/root/generate/exported",
                payload.to_map()?,
            )
            .await
            .with_context(|| "Failed to generate root CA")?
            .unwrap();

        let cert_pem = resp
            .data
            .with_context(|| "Failed to get data from vault request")?
            .get("certificate")
            .and_then(|v| v.as_str())
            .map(|e| e.chars().filter(|c| *c != '\n').collect::<String>())
            .with_context(|| "Failed to get cert pem from vault response")?;

        info!("[vault] generated/exported cluster root CA");
        info!("[vault] cert pem: {cert_pem}");
        Ok(())
    }

    fn generate_join_token(&mut self) -> anyhow::Result<()> {
        let mut bytes = [0_u8; 24];
        rand::rng().fill_bytes(&mut bytes);

        let join_token = base64::encode(bytes);
        info!("[vault] generated join token: {join_token}");

        self.join_token = join_token;
        Ok(())
    }

    pub async fn init(&mut self) -> anyhow::Result<()> {
        info!("[vault] initializing seal configuration");
        let keys = self
            .vault
            .init(&SealConfig {
                secret_shares: 5,
                secret_threshold: 3,
            })
            .await?;
        info!(
            "[vault] initialization complete: shares={}, threshold={}",
            keys.secret_shares.len(),
            3
        );
        let secrets = keys
            .secret_shares
            .iter()
            .map(|e| e.as_slice())
            .collect::<Vec<&[u8]>>();

        for (idx, &key) in secrets.iter().enumerate() {
            info!("<key-{idx}>: {}", base64::encode(key));
        }

        self.vault.unseal(&secrets).await?;
        self.root_token = keys.root_token.clone();
        info!("[vault] unseal succeeded and root token stored in memory");

        self.vault
            .mount(Some(self.root_token.as_str()), "pki", "pki")
            .await?;
        info!("[vault] successfully mounted pki module");

        self.write_policies().await?;
        self.generate_root_ca().await?;
        self.generate_join_token()?;

        info!("[vault] bootstrap sequence completed");

        Ok(())
    }

    pub async fn issue_cert(
        &self,
        role: impl AsRef<str>,
        req: &IssueCertificateRequest,
    ) -> anyhow::Result<IssueCertificateResponse> {
        debug!(
            "[vault] issuing certificate for role={} (cn={:?})",
            role.as_ref(),
            req.common_name
        );
        let data = self
            .vault
            .write(
                Some(self.root_token.as_str()),
                &format!("pki/issue/{}-node", role.as_ref()),
                req.to_map()?,
            )
            .await
            .with_context(|| "Failed to issue certificate")?
            .unwrap()
            .data
            .unwrap();
        let response = serde_json::from_value(Value::Object(data))
            .with_context(|| "Failed to deserialize issue certificate response")
            .inspect(|_| info!("[vault] certificate issued for role={}", role.as_ref()))?;
        Ok(response)
    }
}
