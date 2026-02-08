use anyhow::anyhow;
use rand::{Rng, rngs::OsRng};
use serde_json::{Value, json};
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey};

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::RequestExt,
    storage::StorageEntry,
    utils::seal::{SealBox, SealedSecret},
};

use super::{SSH_CA_PREFIX, SshBackend, SshBackendInner, types::CaGenerateRequest};

impl SshBackend {
    pub fn public_ca_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("public_key")
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.handle_public_ca_read(backend, req).await })
                }
            })
            .help("Get the public key of the default SSH CA (unauthenticated)")
            .build()
    }

    pub fn config_ca_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("config/ca")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.handle_config_ca_write(backend, req).await })
                }
            })
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.handle_config_ca_read(backend, req).await })
                }
            })
            .field(
                "generate",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .description("Generate a new CA key"),
            )
            .field(
                "private_key",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Import existing private key"),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Key type (rsa/ed25519)"),
            )
            .help("Configure or generate SSH CA key")
            .build()
    }

    pub fn ca_generate_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("ca/generate")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.generate_ca(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(false)
                    .default_value("default")
                    .description("CA key name"),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(false)
                    .default_value("ed25519")
                    .description("Key type: ed25519 or rsa"),
            )
            .help("Generate an SSH CA private key and store it")
            .build()
    }
}

impl SshBackendInner {
    pub async fn generate_ca(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: CaGenerateRequest = req.parse_json()?;
        let name = if payload.name.is_empty() {
            "default".to_string()
        } else {
            payload.name
        };
        let key_type = if payload.key_type.is_empty() {
            "ed25519".to_string()
        } else {
            payload.key_type.to_lowercase()
        };

        let pk = match key_type.as_str() {
            "ed25519" => PrivateKey::random(&mut OsRng, Algorithm::Ed25519),
            "rsa" => PrivateKey::random(&mut OsRng, Algorithm::Rsa { hash: None }),
            _ => return Err(RvError::ErrRequestFieldInvalid),
        }?;

        let pem = pk.to_openssh(LineEnding::LF)?;

        let mut key_bytes = [0u8; 32];
        rand::thread_rng().fill(&mut key_bytes);
        let sb = SealBox::new_with_key(pem.as_bytes().to_vec(), key_bytes)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to seal CA key: {}", e)))?;
        let secret = SealedSecret::new(sb, key_bytes);

        let entry = StorageEntry {
            key: format!("{SSH_CA_PREFIX}{name}"),
            value: serde_json::to_vec(&secret)?,
        };
        req.storage_put(&entry).await?;

        let pubkey = pk.public_key().to_openssh()?;
        let resp = json!({
            "name": name,
            "public_key": pubkey,
            // "private_key": *pem, // Don't return private key
        })
        .as_object()
        .ok_or(RvError::ErrResponseDataInvalid)?
        .clone();

        Ok(Some(Response::data_response(Some(resp))))
    }

    pub async fn handle_config_ca_write(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let generate = req
            .get_data("generate")
            .unwrap_or(Value::Bool(false))
            .as_bool()
            .unwrap_or(false);

        let key_str = if generate {
            let key_type = req
                .get_data("key_type")
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or("ed25519".to_string());
            let alg = match key_type.as_str() {
                "rsa" => Algorithm::Rsa {
                    hash: Some(HashAlg::Sha256),
                },
                _ => Algorithm::Ed25519,
            };
            let key = PrivateKey::random(&mut OsRng, alg)
                .map_err(|e| RvError::ErrOther(anyhow!("Failed to generate CA key: {}", e)))?;
            key.to_openssh(LineEnding::LF)?.to_string()
        } else {
            req.get_data("private_key")?
                .as_str()
                .ok_or(RvError::ErrRequestFieldInvalid)?
                .to_string()
        };

        let mut key_bytes = [0u8; 32];
        rand::thread_rng().fill(&mut key_bytes);
        let sb = SealBox::new_with_key(key_str.as_bytes().to_vec(), key_bytes)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to seal CA key: {}", e)))?;
        let secret = SealedSecret::new(sb, key_bytes);

        let entry = StorageEntry {
            key: format!("{SSH_CA_PREFIX}default"),
            value: serde_json::to_vec(&secret)?,
        };
        req.storage_put(&entry).await?;

        let pk = PrivateKey::from_openssh(&key_str)?;
        let pubkey = pk.public_key().to_openssh()?;

        Ok(Some(Response::data_response(Some(
            json!({
                "public_key": pubkey
            })
            .as_object()
            .ok_or(RvError::ErrResponseDataInvalid)?
            .clone(),
        ))))
    }

    pub async fn handle_config_ca_read(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let entry = req.storage_get(&format!("{SSH_CA_PREFIX}default")).await?;
        if let Some(entry) = entry {
            let secret: SealedSecret = serde_json::from_slice(&entry.value)?;
            let key_bytes = secret
                .unwrap_key()
                .map_err(|e| RvError::ErrOther(anyhow!("Failed to unwrap CA key: {}", e)))?;
            let mut sb = secret.box_data;
            sb.unseal_with_key(&key_bytes)
                .map_err(|e| RvError::ErrOther(anyhow!("Failed to unseal CA key: {}", e)))?;
            let key_str =
                String::from_utf8(sb.get().map_err(|_| RvError::ErrPkiInternal)?.clone())?;

            let pk = PrivateKey::from_openssh(&key_str)?;
            let pubkey = pk.public_key().to_openssh()?;
            Ok(Some(Response::data_response(Some(
                json!({
                    "public_key": pubkey
                })
                .as_object()
                .ok_or(RvError::ErrResponseDataInvalid)?
                .clone(),
            ))))
        } else {
            Ok(None)
        }
    }

    pub async fn handle_public_ca_read(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.handle_config_ca_read(_backend, req).await
    }
}
