use crate::protocol::config::{
    config_ref, ip_or_dns, local_alt_names_and_ip_sans, to_alt_names_and_ip_sans,
};
use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use common::{IssueCertificateRequest, RegistryCredential};
use libvault::RustyVault;
use libvault::core::SealConfig;
use libvault::modules::ResponseExt;
use libvault::modules::auth::AuthModule;
use libvault::modules::pki::types::IssueCertificateResponse;
use libvault::storage::Backend;
use libvault::storage::physical::file::FileBackend;
use libvault::storage::xline::{XlineBackend, XlineOptions};
use log::{debug, info, warn};
use serde_json::{Value, json};
use std::fmt::{Display, Formatter};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use x509_parser::prelude::{FromDer, X509Certificate};

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

const RKS_CLIENT_CERT_FILE: &str = "rks-client.pem";
const RKS_CLIENT_KEY_FILE: &str = "rks-client.key";

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
            .map(|&e| BASE64.encode(e))
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

    /// Store a registry credential (PAT) in the KV secret engine.
    /// Path: `secret/registry/{registry_host}`
    ///
    /// The `registry` value is expected to be pre-normalized (lowercased, host:port only).
    pub async fn store_registry_credential(&self, registry: &str, pat: &str) -> anyhow::Result<()> {
        let path = format!("secret/registry/{registry}");
        let data = json!({ "pat": pat, "registry": registry });
        self.vault
            .write(Some(self.root_token.as_str()), &path, data.to_map()?)
            .await
            .with_context(|| format!("Failed to store credential for registry {registry}"))?;
        info!("stored registry credential for {registry}");
        Ok(())
    }

    /// Remove a stored registry credential.
    pub async fn delete_registry_credential(&self, registry: &str) -> anyhow::Result<()> {
        let path = format!("secret/registry/{registry}");
        self.vault
            .delete(Some(self.root_token.as_str()), &path, None)
            .await
            .with_context(|| format!("Failed to delete credential for registry {registry}"))?;
        info!("deleted registry credential for {registry}");
        Ok(())
    }

    /// List all stored registry credentials.
    pub async fn list_registry_credentials(&self) -> anyhow::Result<Vec<RegistryCredential>> {
        let resp = self
            .vault
            .list(Some(self.root_token.as_str()), "secret/registry/")
            .await;

        let keys = match resp {
            Ok(Some(resp)) => resp
                .data
                .and_then(|data| data.get("keys").cloned())
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>(),
            Ok(None) => vec![],
            Err(e) => {
                anyhow::bail!("failed to list registry credential keys from vault: {e}");
            }
        };

        let mut credentials = Vec::with_capacity(keys.len());
        for key in keys {
            let registry = key.trim_end_matches('/');
            match self.get_registry_credential(registry).await {
                Ok(Some(cred)) => credentials.push(cred),
                Ok(None) => {
                    warn!(
                        "registry key '{registry}' listed in vault but has no valid credential; skipping"
                    );
                }
                Err(e) => {
                    warn!("failed to read registry credential for '{registry}': {e}; skipping");
                }
            }
        }
        Ok(credentials)
    }

    /// Get a single registry credential by host.
    pub async fn get_registry_credential(
        &self,
        registry: &str,
    ) -> anyhow::Result<Option<RegistryCredential>> {
        let path = format!("secret/registry/{registry}");
        let resp = self.vault.read(Some(self.root_token.as_str()), &path).await;

        match resp {
            Ok(Some(resp)) => {
                let data = resp.data.unwrap_or_default();
                let pat = data
                    .get("pat")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from);
                let Some(pat) = pat else {
                    warn!("registry credential at '{path}' is missing a non-empty pat");
                    return Ok(None);
                };
                let registry = data
                    .get("registry")
                    .and_then(|v| v.as_str())
                    .unwrap_or(registry)
                    .to_string();
                Ok(Some(RegistryCredential { registry, pat }))
            }
            Ok(None) => Ok(None),
            Err(e) => anyhow::bail!("failed to read registry credential at '{path}': {e}"),
        }
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

    /// Open vault using the file backend directly (no Xline needed).
    /// Requires `rks gen certs` to have been run first.
    pub async fn open_file_backend() -> anyhow::Result<Self> {
        let folder = &config_ref().tls_config.vault_folder;
        if folder.as_os_str().is_empty() {
            anyhow::bail!(
                "vault_folder is empty in config. \
                 Set tls_config.vault_folder and run `rks gen certs` first."
            );
        }

        let keys = extract_keys(&folder.join("keys.json"))
            .await
            .with_context(|| {
                format!(
                    "Cannot read {}/keys.json. Run `rks gen certs <config>` first.",
                    folder.display()
                )
            })?;
        let keys_ref = keys.iter().map(|e| e.as_slice()).collect::<Vec<_>>();

        let mut vault = Vault::with_file_backend()?;
        vault
            .vault
            .unseal(&keys_ref)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to unseal file backend: {e}"))?;

        let root_token = tokio::fs::read_to_string(folder.join("root_token.txt"))
            .await
            .with_context(|| "Failed to read root_token.txt")?;
        vault.root_token = root_token;

        info!("opened vault with file backend at {}", folder.display());
        Ok(vault)
    }

    fn rks_client_cert_path(folder: &Path) -> PathBuf {
        folder.join(RKS_CLIENT_CERT_FILE)
    }

    fn rks_client_key_path(folder: &Path) -> PathBuf {
        folder.join(RKS_CLIENT_KEY_FILE)
    }

    async fn write_rks_client_identity(
        folder: &Path,
        certificate: &str,
        private_key: &str,
    ) -> anyhow::Result<()> {
        tokio::fs::write(Self::rks_client_cert_path(folder), certificate).await?;
        tokio::fs::write(Self::rks_client_key_path(folder), private_key).await?;
        Ok(())
    }

    fn cached_rks_client_identity_needs_refresh(cert_pem: &str) -> anyhow::Result<bool> {
        let mut reader = Cursor::new(cert_pem.as_bytes());
        let certs = rustls_pemfile::certs(&mut reader)
            .collect::<std::io::Result<Vec<_>>>()
            .with_context(|| "Failed to extract cached rks client certificate from PEM chain")?;
        let cert = certs
            .first()
            .with_context(|| "No valid certs found in cached rks client certificate")?;
        let (_, parsed) = X509Certificate::from_der(cert.as_ref()).map_err(|err| {
            anyhow::anyhow!("Failed to parse cached rks client certificate: {err}")
        })?;
        Ok(parsed.validity().not_after.to_datetime() <= time::OffsetDateTime::now_utc())
    }

    async fn ensure_rks_client_identity() -> anyhow::Result<()> {
        let folder = &config_ref().tls_config.vault_folder;
        if folder.as_os_str().is_empty() {
            anyhow::bail!(
                "vault_folder is empty in config. \
                 Set tls_config.vault_folder and run `rks gen certs` first."
            );
        }

        let cert_exists = tokio::fs::try_exists(Self::rks_client_cert_path(folder))
            .await
            .unwrap_or(false);
        let key_exists = tokio::fs::try_exists(Self::rks_client_key_path(folder))
            .await
            .unwrap_or(false);
        if cert_exists && key_exists {
            let cert_path = Self::rks_client_cert_path(folder);
            match tokio::fs::read_to_string(&cert_path).await {
                Ok(cert_pem) => match Self::cached_rks_client_identity_needs_refresh(&cert_pem) {
                    Ok(false) => return Ok(()),
                    Ok(true) => {
                        info!(
                            "cached rks xline client identity at {} is expired; reissuing",
                            cert_path.display()
                        );
                    }
                    Err(err) => {
                        warn!(
                            "failed to inspect cached rks xline client identity at {}: {}; reissuing",
                            cert_path.display(),
                            err
                        );
                    }
                },
                Err(err) => {
                    warn!(
                        "failed to read cached rks xline client certificate at {}: {}; reissuing",
                        cert_path.display(),
                        err
                    );
                }
            }
        }

        let keys = extract_keys(&folder.join("keys.json"))
            .await
            .with_context(|| {
                format!(
                    "Cannot read {}/keys.json. Run `rks gen certs <config>` first.",
                    folder.display()
                )
            })?;
        let keys_ref = keys.iter().map(|e| e.as_slice()).collect::<Vec<_>>();

        let mut vault = Vault::with_file_backend()?;
        vault
            .vault
            .unseal(&keys_ref)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to unseal file backend: {e}"))?;

        vault.root_token = tokio::fs::read_to_string(folder.join("root_token.txt"))
            .await
            .with_context(|| "Failed to read root_token.txt")?;

        let resp = vault.issue_rks_cert().await?;
        Self::write_rks_client_identity(folder, &resp.certificate, &resp.private_key).await?;
        info!("stored rks xline client identity at {}", folder.display());
        Ok(())
    }

    pub async fn xline_options_for_rks() -> anyhow::Result<XlineOptions> {
        let folder = &config_ref().tls_config.vault_folder;
        if folder.as_os_str().is_empty() {
            anyhow::bail!(
                "vault_folder is empty in config. \
                 Set tls_config.vault_folder and run `rks gen certs` first."
            );
        }

        let root_cert = tokio::fs::read_to_string(folder.join("root.pem"))
            .await
            .with_context(|| "Failed to read root.pem")?;
        let certificate = tokio::fs::read_to_string(Self::rks_client_cert_path(folder))
            .await
            .with_context(|| format!("Failed to read {}", RKS_CLIENT_CERT_FILE))?;
        let private_key = tokio::fs::read_to_string(Self::rks_client_key_path(folder))
            .await
            .with_context(|| format!("Failed to read {}", RKS_CLIENT_KEY_FILE))?;

        let cfg = config_ref();
        let mut xline_options = XlineOptions::new(cfg.xline_config.endpoints.clone());
        xline_options = xline_options.with_tls(&root_cert, &certificate, &private_key)?;
        Ok(xline_options)
    }

    pub async fn open_xline_backend() -> anyhow::Result<Self> {
        let folder = &config_ref().tls_config.vault_folder;
        if folder.as_os_str().is_empty() {
            anyhow::bail!(
                "vault_folder is empty in config. \
                 Set tls_config.vault_folder and run `rks gen certs` first."
            );
        }

        let keys = extract_keys(&folder.join("keys.json"))
            .await
            .with_context(|| {
                format!(
                    "Cannot read {}/keys.json. Run `rks gen certs <config>` first.",
                    folder.display()
                )
            })?;
        let keys_ref = keys.iter().map(|e| e.as_slice()).collect::<Vec<_>>();

        let xline_backend = Arc::new(XlineBackend::with_options(
            Self::xline_options_for_rks().await?,
        ));
        let mut vault = Vault::new(xline_backend)?;
        let inited = vault
            .vault
            .inited()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query xline-backed vault state: {e}"))?;
        if !inited {
            anyhow::bail!("xline-backed vault is not initialized");
        }

        vault
            .vault
            .unseal(&keys_ref)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to unseal xline backend: {e}"))?;

        let root_token = tokio::fs::read_to_string(folder.join("root_token.txt"))
            .await
            .with_context(|| "Failed to read root_token.txt")?;
        vault.root_token = root_token;

        info!("opened vault with xline backend");
        Ok(vault)
    }

    /// Open vault: tries Xline migration when TLS is enabled,
    /// falls back to file backend otherwise.
    pub async fn open() -> anyhow::Result<Self> {
        if config_ref().tls_config.enable {
            Self::ensure_rks_client_identity().await?;
            match Self::open_xline_backend().await {
                Ok(vault) => Ok(vault),
                Err(err) if err.to_string().contains("not initialized") => Self::migrate().await,
                Err(err) => Err(err),
            }
        } else {
            Self::open_file_backend().await
        }
    }

    pub async fn migrate() -> anyhow::Result<Self> {
        info!("preparing to migrate from file backend");

        let folder = &config_ref().tls_config.vault_folder;

        let keys = extract_keys(&folder.join("keys.json")).await?;
        let keys_ref = keys.iter().map(|e| e.as_slice()).collect::<Vec<_>>();

        if !config_ref().tls_config.keep_dangerous_files {
            warn!(
                "keeping keys.json despite keep_dangerous_files=false because \
                 reopening the xline-backed vault still requires the unseal keys"
            );
        }

        info!("migration stage1: prepare file backend and client identity");
        let mut vault = Vault::with_file_backend()?;

        vault.vault.unseal(&keys_ref).await?;

        let root_token = tokio::fs::read_to_string(folder.join("root_token.txt")).await?;
        vault.root_token = root_token.clone();

        Self::ensure_rks_client_identity().await?;
        let xline_options = Self::xline_options_for_rks().await?;

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
                .map(|e| BASE64.decode(e).unwrap())
                .collect::<Vec<_>>()
        })
        .with_context(|| "keys.json doesn't contain a key named with `keys`")
}

#[cfg(test)]
mod tests {
    use super::*;
    use libvault::modules::ResponseExt;
    use libvault::storage::physical::file::FileBackend;
    use rcgen::{Certificate, CertificateParams};
    use time::{Duration, OffsetDateTime};

    /// Create a standalone vault with file backend for testing.
    /// No external services (Xline) needed.
    async fn setup_test_vault() -> anyhow::Result<(Vault, tempfile::TempDir)> {
        let temp_dir = tempfile::tempdir()?;
        let backend = FileBackend::with_folder(temp_dir.path())?;
        let mut vault = Vault::new(Arc::new(backend))?;

        let keys = vault
            .vault
            .init(&SealConfig {
                secret_shares: 1,
                secret_threshold: 1,
            })
            .await
            .map_err(|e| anyhow::anyhow!("init failed: {e}"))?;

        vault
            .vault
            .unseal(&[keys.secret_shares[0].as_slice()])
            .await
            .map_err(|e| anyhow::anyhow!("unseal failed: {e}"))?;

        vault.root_token = keys.root_token.clone();
        Ok((vault, temp_dir))
    }

    #[tokio::test]
    async fn test_store_and_get_registry_credential() {
        let (vault, _dir) = setup_test_vault().await.unwrap();

        vault
            .store_registry_credential("libra.tools", "test-token-123")
            .await
            .unwrap();

        let cred = vault
            .get_registry_credential("libra.tools")
            .await
            .unwrap()
            .expect("credential should exist");

        assert_eq!(cred.registry, "libra.tools");
        assert_eq!(cred.pat, "test-token-123");
    }

    #[tokio::test]
    async fn test_list_registry_credentials() {
        let (vault, _dir) = setup_test_vault().await.unwrap();

        vault
            .store_registry_credential("libra.tools", "token1")
            .await
            .unwrap();
        vault
            .store_registry_credential("other.registry.io", "token2")
            .await
            .unwrap();

        let creds = vault.list_registry_credentials().await.unwrap();
        assert_eq!(creds.len(), 2);

        let registries: Vec<&str> = creds.iter().map(|c| c.registry.as_str()).collect();
        assert!(registries.contains(&"libra.tools"));
        assert!(registries.contains(&"other.registry.io"));
    }

    #[tokio::test]
    async fn test_delete_registry_credential() {
        let (vault, _dir) = setup_test_vault().await.unwrap();

        vault
            .store_registry_credential("libra.tools", "token")
            .await
            .unwrap();

        vault
            .delete_registry_credential("libra.tools")
            .await
            .unwrap();

        let cred = vault.get_registry_credential("libra.tools").await.unwrap();
        assert!(cred.is_none(), "credential should be deleted");
    }

    #[tokio::test]
    async fn test_list_empty_credentials() {
        let (vault, _dir) = setup_test_vault().await.unwrap();

        let creds = vault.list_registry_credentials().await.unwrap();
        assert!(creds.is_empty());
    }

    #[tokio::test]
    async fn test_overwrite_registry_credential() {
        let (vault, _dir) = setup_test_vault().await.unwrap();

        vault
            .store_registry_credential("libra.tools", "old-token")
            .await
            .unwrap();
        vault
            .store_registry_credential("libra.tools", "new-token")
            .await
            .unwrap();

        let cred = vault
            .get_registry_credential("libra.tools")
            .await
            .unwrap()
            .expect("credential should exist");
        assert_eq!(cred.pat, "new-token");

        let creds = vault.list_registry_credentials().await.unwrap();
        assert_eq!(creds.len(), 1);
    }

    #[tokio::test]
    async fn test_get_registry_credential_surfaces_backend_read_errors() {
        let (mut vault, _dir) = setup_test_vault().await.unwrap();

        vault
            .store_registry_credential("libra.tools", "token")
            .await
            .unwrap();

        vault.root_token = "bad-token".to_string();

        let err = vault
            .get_registry_credential("libra.tools")
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("failed to read registry credential at 'secret/registry/libra.tools'"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn test_list_registry_credentials_skips_invalid_entry() {
        let (vault, _dir) = setup_test_vault().await.unwrap();

        vault
            .store_registry_credential("libra.tools", "token")
            .await
            .unwrap();

        vault
            .vault
            .write(
                Some(vault.root_token.as_str()),
                "secret/registry/bad.registry",
                serde_json::json!({
                    "registry": "bad.registry",
                })
                .to_map()
                .unwrap(),
            )
            .await
            .unwrap();

        let creds = vault.list_registry_credentials().await.unwrap();
        assert_eq!(
            creds.len(),
            1,
            "should skip the invalid entry and return only the valid one"
        );
        assert_eq!(creds[0].registry, "libra.tools");
    }

    fn issue_test_cert(
        not_before: OffsetDateTime,
        not_after: OffsetDateTime,
    ) -> anyhow::Result<String> {
        let mut params = CertificateParams::new(vec!["rks-node".to_string()]);
        params.not_before = not_before;
        params.not_after = not_after;
        Ok(Certificate::from_params(params)?.serialize_pem()?)
    }

    #[test]
    fn cached_rks_client_identity_stays_when_certificate_is_valid() {
        let now = OffsetDateTime::now_utc();
        let cert_pem = issue_test_cert(now - Duration::days(1), now + Duration::days(30)).unwrap();

        assert!(!Vault::cached_rks_client_identity_needs_refresh(&cert_pem).unwrap());
    }

    #[test]
    fn cached_rks_client_identity_refreshes_when_certificate_is_expired() {
        let now = OffsetDateTime::now_utc();
        let cert_pem = issue_test_cert(now - Duration::days(30), now - Duration::days(1)).unwrap();

        assert!(Vault::cached_rks_client_identity_needs_refresh(&cert_pem).unwrap());
    }
}
