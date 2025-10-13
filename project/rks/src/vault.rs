use crate::protocol::config::config;
use anyhow::Context;
use derive_more::Deref;
use libvault::RustyVault;
use libvault::core::SealConfig;
use libvault::modules::ResponseExt;
use libvault::modules::pki::types::{IssueCertificateRequest, IssueCertificateResponse};
use libvault::storage::Backend;
use libvault::storage::physical::file::FileBackend;
use libvault::storage::xline::XlineBackend;
use log::{debug, info};
use rand::RngCore;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

#[derive(Deref)]
pub struct Vault {
    #[deref]
    vault: RustyVault,
    root_token: String,
    join_token: String,
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

    pub fn with_file_backend() -> anyhow::Result<Self> {
        let path = &config().tls_config.vault_folder;

        let backend = FileBackend::with_folder(path)?;
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
            "ttl": "180d",
            "max_ttl": "360d",
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
            "ttl": "180d",
            "max_ttl": "360d",
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

        let xline_role = json!({
            "key_type": "ec",
            "key_bits": 256,
            "allow_ip_sans": true,
            "allowed_domains": "xline.svc.cluster.local",
            "allow_subdomains": true,
            "server_flag": true,
            "client_flag": true,
            "ttl": "180d",
            "max_ttl": "360d",
            "no_store": false,
            "generate_lease": false,
        });

        self.vault
            .write(
                Some(self.root_token.as_str()),
                "pki/roles/xline-node",
                xline_role.to_map()?,
            )
            .await
            .with_context(|| "Failed to write policy")?;
        info!("[vault] published role policy pki/roles/xline-node");
        Ok(())
    }

    async fn generate_root_ca(&self, folder: impl AsRef<Path>) -> anyhow::Result<()> {
        let folder = folder.as_ref();

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
            "permitted_dns_domains": "rkl.svc.cluster.local,rks.svc.cluster.local,xline.svc.cluster.local",
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

        let data = resp
            .data
            .with_context(|| "Failed to get data from vault request")?;

        let cert_pem = data
            .get("certificate")
            .and_then(|v| v.as_str())
            .with_context(|| "Failed to get cert pem from vault response")?;
        let private_key = data
            .get("private_key")
            .and_then(|v| v.as_str())
            .with_context(|| "Failed to get private key of root ca from vault response")?;

        info!("[vault] generated/exported cluster root CA");

        tokio::fs::write(folder.join("root.pem"), cert_pem).await?;
        tokio::fs::write(folder.join("private.key"), private_key).await?;

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

    async fn write_keys_and_root_token(
        &self,
        folder: impl AsRef<Path>,
        secrets: &[&[u8]],
    ) -> anyhow::Result<()> {
        let folder = folder.as_ref();

        let keys = secrets
            .into_iter()
            .map(|&e| base64::encode(e))
            .collect::<Vec<_>>();

        let keys_path = folder.join("keys.json");
        let keys_json = serde_json::to_string_pretty(&json!({
            "keys": keys,
        }))?;
        tokio::fs::write(keys_path, keys_json.as_bytes()).await?;

        let root_token_path = folder.join("root_token.txt");
        tokio::fs::write(root_token_path, self.root_token.as_bytes()).await?;
        Ok(())
    }

    async fn write_certs_for_xlines(&self, folder: impl AsRef<Path>) -> anyhow::Result<()> {
        let folder = folder.as_ref();

        for addr in &config().xline_config.endpoints {
            let mut alt_names = None;
            let mut ip_sans = None;

            match addr.parse::<SocketAddr>() {
                Ok(addr) => ip_sans = Some(addr.ip().to_string()),
                Err(_) => {
                    alt_names = {
                        let (host, _) = addr.rsplit_once(":").unwrap_or((addr, ""));
                        Some(host.to_string())
                    }
                }
            }

            let req = IssueCertificateRequest {
                common_name: "xline-cluster".to_string().into(),
                alt_names,
                ip_sans,
                ttl: "180d".to_string().into(),
            };

            let safe_addr = addr
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .replace(":", "-");
            let folder = folder.join(format!("xline-{safe_addr}"));
            tokio::fs::create_dir(&folder).await?;

            let res = self.issue_cert("xline", &req).await?;

            tokio::fs::write(folder.join("cert.pem"), &res.certificate).await?;
            tokio::fs::write(folder.join("private.key"), &res.private_key).await?;
        }

        info!(
            target: "rks::vault",
            "successfully wrote all certificates to xline cluster",
        );
        Ok(())
    }

    pub async fn generate_certs(&mut self) -> anyhow::Result<()> {
        info!(
            target: "rks::vault",
            "initializing seal configuration"
        );

        let keys = self
            .vault
            .init(&SealConfig {
                secret_shares: 5,
                secret_threshold: 3,
            })
            .await?;

        info!(
            target: "rks::vault",
            "initialization complete: shares={}, threshold={}",
            keys.secret_shares.len(),
            3
        );

        let secrets = keys
            .secret_shares
            .iter()
            .map(|e| e.as_slice())
            .collect::<Vec<&[u8]>>();

        self.vault.unseal(&secrets).await?;
        self.root_token = keys.root_token.clone();
        info!(
            target: "rks::vault",
            "unseal succeeded and root token stored in memory"
        );

        self.vault
            .mount(self.root_token.as_str().into(), "pki", "pki")
            .await?;

        let folder = &config().tls_config.vault_folder;

        self.write_keys_and_root_token(folder, &secrets).await?;
        self.generate_root_ca(folder).await?;
        self.write_policies().await?;
        self.write_certs_for_xlines(folder).await?;

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

    pub async fn migrate() -> anyhow::Result<Self> {
        info!(
            target: "rks::vault",
            "preparing to migrate from file backend",
        );

        let mut vault = Vault::new()?;
        let folder = &config().tls_config.vault_folder;

        let source = FileBackend::with_folder(folder)?;
        let backend = Arc::new(source);

        vault.vault.core.load().migrate(backend).await
            .with_context(|| "Failed to migrate from file backend")?;

        let keys = serde_json::from_str::<Value>(
            &tokio::fs::read_to_string(folder.join("keys.json")).await?,
        )?;
        let keys = keys
            .as_object()
            .unwrap()
            .get("keys")
            .and_then(|v| v.as_array())
            .map(|v| {
                v.into_iter()
                    .map(|e| e.as_str().map(|e| e.as_bytes()).unwrap())
                    .map(|e| base64::decode(e).unwrap())
                    .collect::<Vec<_>>()
            })
            .with_context(|| "keys.json doesn't contain a key named with `keys`")?;
        let keys_ref = keys.iter()
            .map(|e| e.as_slice())
            .collect::<Vec<_>>();

        vault.vault.unseal(&keys_ref).await
            .with_context(|| "Failed to unseal vault")?;

        let root_token = tokio::fs::read_to_string(folder.join("root_token.txt")).await?;
        vault.root_token = root_token;

        info!(
            target: "rks::vault",
            "successfully migrated from file backend",
        );
        Ok(vault)
    }
}
