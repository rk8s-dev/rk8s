use crate::protocol::config::{
    config_ref, ip_or_dns, local_alt_names_and_ip_sans, to_alt_names_and_ip_sans,
};
use anyhow::Context;
use common::IssueCertificateRequest;
use libvault::RustyVault;
use libvault::core::SealConfig;
use libvault::modules::ResponseExt;
use libvault::modules::auth::AuthModule;
use libvault::modules::pki::types::IssueCertificateResponse;
use libvault::storage::Backend;
use libvault::storage::physical::file::FileBackend;
use libvault::storage::xline::{XlineBackend, XlineOptions};
use log::{debug, info};
use serde_json::{Value, json};
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub enum CertRole {
    Rks,
    Rkl,
    Xline,
}

impl Display for CertRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rks => write!(f, "rks"),
            Self::Rkl => write!(f, "rkl"),
            Self::Xline => write!(f, "xline"),
        }
    }
}

pub struct Vault {
    vault: RustyVault,
    root_token: String,
}

impl Vault {
    pub fn new(backend: Arc<dyn Backend>) -> anyhow::Result<Self> {
        Ok(Self {
            vault: RustyVault::new(backend, None)?,
            root_token: String::new(),
        })
    }

    pub fn with_file_backend() -> anyhow::Result<Self> {
        let path = &config_ref().tls_config.vault_folder;
        let backend = FileBackend::with_folder(path)?;
        Self::new(Arc::new(backend))
    }

    pub async fn write_policy(&self, cert: CertRole) -> anyhow::Result<()> {
        let role = json!({
            "key_type": "ec",
            "key_bits": 256,
            "allow_ip_sans": true,
            "allowed_domains": format!("{cert}.svc.cluster.local"),
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
                Some(self.root_token.to_string()),
                format!("pki/roles/{cert}-node"),
                role.to_map()?,
            )
            .await
            .with_context(|| "Failed to write policy")?;
        info!("Published role policy pki/roles/{cert}-node");
        Ok(())
    }

    async fn write_policies(&self) -> anyhow::Result<()> {
        self.write_policy(CertRole::Rks).await?;
        self.write_policy(CertRole::Rkl).await?;
        self.write_policy(CertRole::Xline).await
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
            "permitted_dns_domains": "rkl.svc.cluster.local,rks.svc.cluster.local,xline.svc.cluster.local,vault.svc.cluster.local",
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

        info!("generated/exported cluster root CA");

        tokio::fs::write(folder.join("root.pem"), cert_pem).await?;
        tokio::fs::write(folder.join("root.key"), private_key).await?;

        Ok(())
    }

    async fn write_keys_and_root_token(
        &self,
        folder: impl AsRef<Path>,
        secrets: &[&[u8]],
    ) -> anyhow::Result<()> {
        let folder = folder.as_ref();

        let keys = secrets
            .iter()
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

        for addr in &config_ref().xline_config.endpoints {
            let (alt_names, ip_sans) = to_alt_names_and_ip_sans(ip_or_dns(addr));

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

            let res = self.issue_cert(CertRole::Xline, &req).await?;

            tokio::fs::write(folder.join("cert.pem"), &res.certificate).await?;
            tokio::fs::write(folder.join("private.key"), &res.private_key).await?;
        }

        info!("successfully wrote all certificates to xline cluster");
        Ok(())
    }

    pub async fn generate_certs(&mut self) -> anyhow::Result<()> {
        info!("initializing seal configuration");

        let keys = self
            .vault
            .init(&SealConfig {
                secret_shares: 5,
                secret_threshold: 3,
            })
            .await?;

        info!(
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
        info!("unseal succeeded and root token stored in memory");

        self.vault
            .mount(self.root_token.as_str().into(), "pki", "pki")
            .await?;

        let folder = &config_ref().tls_config.vault_folder;

        self.write_keys_and_root_token(folder, &secrets).await?;
        self.generate_root_ca(folder).await?;
        self.write_policies().await?;
        self.write_certs_for_xlines(folder).await?;

        Ok(())
    }

    pub async fn generate_once_token(&self) -> anyhow::Result<String> {
        let payload = json!({
            "policies": ["default"],
            "display_name": "one-shot",
            "num_uses": 1,
        })
        .as_object()
        .cloned();

        let resp = self
            .vault
            .write(
                self.root_token.as_str().into(),
                "auth/token/create",
                payload,
            )
            .await?;
        resp.and_then(|r| r.auth)
            .map(|auth| auth.client_token)
            .with_context(|| "Failed to generate token")
    }

    pub async fn validate_token(&self, token: impl AsRef<str>) -> anyhow::Result<()> {
        let token = token.as_ref();

        let auth_module = self
            .vault
            .core
            .load()
            .module_manager
            .get_module::<AuthModule>("auth")
            .unwrap();

        let token_store = auth_module.token_store.load().as_ref().unwrap().clone();
        token_store
            .check_token("no-used", token)
            .await?
            .ok_or_else(|| anyhow::anyhow!("The token is valid"))?;
        Ok(())
    }

    pub async fn issue_cert(
        &self,
        role: CertRole,
        req: &IssueCertificateRequest,
    ) -> anyhow::Result<IssueCertificateResponse> {
        debug!(
            "issuing certificate for role={role} (cn={:?})",
            req.common_name
        );

        let request = req.clone();

        let data = self
            .vault
            .write(
                Some(self.root_token.as_str()),
                &format!("pki/issue/{}-node", role),
                request.to_map()?,
            )
            .await
            .with_context(|| "Failed to issue certificate")?
            .unwrap()
            .data
            .unwrap();

        let response = serde_json::from_value(Value::Object(data))
            .with_context(|| "Failed to deserialize issue certificate response")
            .inspect(|_| info!("certificate issued for role={role}"))?;
        Ok(response)
    }

    pub async fn issue_rks_cert(&self) -> anyhow::Result<IssueCertificateResponse> {
        let (alt_names, ip_sans) = local_alt_names_and_ip_sans();
        let req = IssueCertificateRequest {
            common_name: "rks-node".to_string().into(),
            alt_names,
            ip_sans,
            ttl: "180d".to_string().into(),
        };
        self.issue_cert(CertRole::Rks, &req).await
    }

    pub async fn migrate() -> anyhow::Result<Self> {
        info!("preparing to migrate from file backend");

        let folder = &config_ref().tls_config.vault_folder;

        let keys = extract_keys(&folder.join("keys.json")).await?;
        let keys_ref = keys.iter().map(|e| e.as_slice()).collect::<Vec<_>>();

        if !config_ref().tls_config.keep_dangerous_files {
            tokio::fs::remove_file(folder.join("keys.json")).await?;
        }

        info!("migration stage1: generate certificates for vault");
        let mut vault = Vault::with_file_backend()?;

        vault.vault.unseal(&keys_ref).await?;

        let root_cert = tokio::fs::read_to_string(folder.join("root.pem")).await?;
        let root_token = tokio::fs::read_to_string(folder.join("root_token.txt")).await?;
        vault.root_token = root_token.clone();

        let (alt_names, ip_sans) = local_alt_names_and_ip_sans();
        let req = IssueCertificateRequest {
            common_name: "rks-cluster".to_string().into(),
            alt_names,
            ip_sans,
            ttl: "180d".to_string().into(),
        };

        let resp = vault.issue_cert(CertRole::Rks, &req).await?;
        let cfg = config_ref();
        let mut xline_options = XlineOptions::new(cfg.xline_config.endpoints.clone());
        xline_options = xline_options.with_tls(&root_cert, &resp.certificate, &resp.private_key)?;

        info!("migration stage2: connect to xline backend and do migration");
        let xline_backend = Arc::new(XlineBackend::with_options(xline_options));
        let mut vault = Vault::new(xline_backend)?;

        let source = FileBackend::with_folder(folder)?;
        let source_backend = Arc::new(source);

        vault
            .vault
            .core
            .load()
            .migrate(source_backend)
            .await
            .with_context(|| "Failed to migrate from file backend")?;

        vault
            .vault
            .unseal(&keys_ref)
            .await
            .with_context(|| "Failed to unseal vault")?;

        vault.root_token = root_token;

        info!("successfully migrated from file backend");
        Ok(vault)
    }
}

async fn extract_keys(path: &Path) -> anyhow::Result<Vec<Vec<u8>>> {
    let keys = serde_json::from_str::<Value>(&tokio::fs::read_to_string(path).await?)?;
    keys.as_object()
        .unwrap()
        .get("keys")
        .and_then(|v| v.as_array())
        .map(|v| {
            v.iter()
                .map(|e| e.as_str().map(|e| e.as_bytes()).unwrap())
                .map(|e| base64::decode(e).unwrap())
                .collect::<Vec<_>>()
        })
        .with_context(|| "keys.json doesn't contain a key named with `keys`")
}
