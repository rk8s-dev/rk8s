use anyhow::anyhow;
use rand::{Rng, rngs::OsRng};
use serde_json::json;
use ssh_key::{Algorithm, Fingerprint, HashAlg, LineEnding, PrivateKey};

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::RequestExt,
    storage::StorageEntry,
    utils::seal::{SealBox, SealedSecret},
};

use super::{SSH_KEYS_PREFIX, SshBackend, SshBackendInner, types::SshKeyGenerateRequest};

impl SshBackend {
    pub fn key_generate_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("keys/generate")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.generate_key(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("User key name"),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(false)
                    .default_value("ed25519")
                    .description("Key type: ed25519 or rsa"),
            )
            .field(
                "comment",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(false)
                    .default_value("")
                    .description("Comment attached to the public key"),
            )
            .help("Generate an SSH user private key and store it")
            .build()
    }

    pub fn keys_list_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("keys/?")
            .operation(Operation::List, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.list_keys(backend, req).await })
                }
            })
            .help("List generated SSH keys")
            .build()
    }
}

impl SshBackendInner {
    pub async fn generate_key(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: SshKeyGenerateRequest = req.parse_json()?;
        let alg = match payload.key_type.as_str() {
            "rsa" => Algorithm::Rsa {
                hash: Some(HashAlg::Sha256),
            },
            "ed25519" => Algorithm::Ed25519,
            _ => Algorithm::Ed25519,
        };

        let mut key = PrivateKey::random(&mut OsRng, alg)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to generate key: {}", e)))?;

        if let Some(pass) = payload.passphrase.as_ref().filter(|p| !p.is_empty()) {
            key = key
                .encrypt(&mut OsRng, pass)
                .map_err(|e| RvError::ErrOther(anyhow!("Failed to encrypt key: {}", e)))?;
        }

        let key_str = key.to_openssh(LineEnding::LF)?;

        // Seal the key using SealBox
        let mut key_bytes = [0u8; 32];
        rand::thread_rng().fill(&mut key_bytes);

        let sb = SealBox::new_with_key(key_str.as_bytes().to_vec(), key_bytes)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to seal key: {}", e)))?;

        let secret = SealedSecret::new(sb, key_bytes);

        let entry = StorageEntry {
            key: format!("{SSH_KEYS_PREFIX}{}", payload.key_name),
            value: serde_json::to_vec(&secret)?,
        };
        req.storage_put(&entry).await?;

        let public = key.public_key();
        let fp = Fingerprint::new(HashAlg::Sha256, public.key_data());
        let pubkey = public.to_openssh()?;

        let resp = json!({
            "name": payload.key_name,
            "private_key": key_str.as_str(),
            "public_key": pubkey,
            "fingerprint": fp.to_string(),
        });

        Ok(Some(Response::data_response(Some(
            resp.as_object()
                .ok_or(RvError::ErrResponseDataInvalid)?
                .clone(),
        ))))
    }

    pub async fn list_keys(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let keys = req.storage_list(SSH_KEYS_PREFIX).await?;
        Ok(Some(Response::list_response(&keys)))
    }
}
