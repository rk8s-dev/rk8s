use std::{any::Any, fmt, sync::Arc};

use anyhow::anyhow;
use async_trait::async_trait;
use derive_more::Deref;
use pgp::composed::{Deserializable, SignedSecretKey};
use zeroize::Zeroizing;

use crate::{
    core::Core,
    errors::RvError,
    logical::{Backend, LogicalBackend, Request},
    modules::Module,
    utils::seal::SealedSecret,
};

use self::path_revoke::PGP_REVOKED_PREFIX;

pub mod path_decrypt;
pub mod path_delete;
pub mod path_encrypt;
pub mod path_export;
pub mod path_generate;
pub mod path_import;
pub mod path_list;
pub mod path_revoke;
pub mod path_sign;
pub mod path_verify;
pub mod types;

static PGP_BACKEND_HELP: &str = r#"Generate and store OpenPGP (PGP/GnuPG) keys and certificates."#;

pub(crate) const PGP_KEYS_PREFIX: &str = "pgp/keys/";

#[derive(Debug, Clone)]
pub struct OpenPgpModule {
    pub name: String,
    pub backend: Arc<OpenPgpBackend>,
}

pub struct OpenPgpBackendInner {
    pub core: Arc<Core>,
}

impl fmt::Debug for OpenPgpBackendInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenPgpBackendInner").finish()
    }
}

#[derive(Clone, Deref)]
pub struct OpenPgpBackend {
    #[deref]
    pub inner: Arc<OpenPgpBackendInner>,
}

impl fmt::Debug for OpenPgpBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenPgpBackend").finish()
    }
}

impl OpenPgpBackend {
    pub fn new(core: Arc<Core>) -> Self {
        Self {
            inner: Arc::new(OpenPgpBackendInner { core }),
        }
    }

    pub fn new_backend(&self) -> LogicalBackend {
        let builder = LogicalBackend::builder()
            .help(PGP_BACKEND_HELP)
            .root_paths(["pgp/*"])
            .path(self.generate_path())
            .path(self.fetch_path())
            .path(self.list_path())
            .path(self.import_path())
            .path(self.sign_path())
            .path(self.verify_path())
            .path(self.encrypt_path())
            .path(self.decrypt_path())
            .path(self.export_path())
            .path(self.delete_path())
            .path(self.revoke_path());

        builder.build()
    }
}

impl OpenPgpBackendInner {
    pub(crate) async fn get_private_key(
        &self,
        req: &mut Request,
        name: &str,
    ) -> Result<SignedSecretKey, RvError> {
        // 1. Validate name input
        if name.trim().is_empty()
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(RvError::ErrRequestFieldInvalid);
        }

        // 2. Check if revoked
        if req
            .storage_get(&format!("{}{}", PGP_REVOKED_PREFIX, name))
            .await?
            .is_some()
        {
            return Err(RvError::ErrPkiCertRevoked);
        }

        let entry = req
            .storage_get(&format!("{}{}", PGP_KEYS_PREFIX, name))
            .await?
            .ok_or(RvError::ErrPkiCertNotFound)?;

        // 3. Enforce encrypted storage (Remove plaintext fallback)
        let secret: SealedSecret =
            serde_json::from_slice(&entry.value).map_err(|_| RvError::ErrPkiDataInvalid)?; // Fail if not encrypted SealedSecret

        let key_bytes = secret
            .unwrap_key()
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to unwrap key: {}", e)))?;
        let mut sb = secret.box_data;
        sb.unseal_with_key(&key_bytes)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to unseal key: {}", e)))?;

        // 4. Secure string handling (Minimize clone/exposure)
        // Note: sb.get() returns reference. We must clone to create String, but we should zeroize if possible.
        // Rust's String doesn't zeroize on drop by default.
        // We use Zeroizing<String> if we could change return type, but here we just ensure we don't leave plain string in "value" if possible.
        // However, SignedSecretKey::from_string takes &str.

        let secret_str = Zeroizing::new(
            String::from_utf8(sb.get().map_err(|_| RvError::ErrPkiInternal)?.clone())
                .map_err(|e| RvError::ErrOther(anyhow!("Invalid UTF-8 key data: {}", e)))?,
        );

        let (secret_key, _) = SignedSecretKey::from_string(&secret_str)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to load secret key: {}", e)))?;

        Ok(secret_key)
    }
}

impl OpenPgpModule {
    pub fn new(core: Arc<Core>) -> Self {
        Self {
            name: "openpgp".to_string(),
            backend: Arc::new(OpenPgpBackend::new(core)),
        }
    }
}

#[async_trait]
impl Module for OpenPgpModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn setup(&self, core: &Core) -> Result<(), RvError> {
        let pgp = self.backend.clone();
        let new_func = move |_c: Arc<Core>| -> Result<Arc<dyn Backend>, RvError> {
            let mut b = pgp.new_backend();
            b.init()?;
            Ok(Arc::new(b))
        };
        core.add_logical_backend("openpgp", Arc::new(new_func))
    }
}

#[cfg(test)]
mod tests {
    use pgp::composed::KeyType;
    use pgp::{
        composed::SecretKeyParamsBuilder,
        crypto::{hash::HashAlgorithm, sym::SymmetricKeyAlgorithm},
        types::{CompressionAlgorithm, SecretKeyTrait},
    };
    use rand::thread_rng;

    #[test]
    fn generate_pgp_keypair() {
        let mut key_params = SecretKeyParamsBuilder::default();
        key_params
            .key_type(KeyType::Rsa(2048))
            .can_certify(true)
            .can_sign(true)
            .primary_user_id("Test User <test@example.com>".into())
            .preferred_symmetric_algorithms(vec![SymmetricKeyAlgorithm::AES256].into())
            .preferred_hash_algorithms(vec![HashAlgorithm::SHA2_256].into())
            .preferred_compression_algorithms(vec![CompressionAlgorithm::ZLIB].into());

        let secret_key_params = key_params.build().unwrap();
        let secret_key = secret_key_params.generate(thread_rng()).unwrap();

        let signed_secret_key = secret_key.sign(thread_rng(), || String::new()).unwrap();
        let _public_key = signed_secret_key.public_key();

        // let _pub_armored = public_key
        //     .to_armored_string(ArmorOptions::default())
        //     .unwrap();
        // let _sec_armored = signed_secret_key
        //     .to_armored_string(ArmorOptions::default())
        //     .unwrap();
    }
}
