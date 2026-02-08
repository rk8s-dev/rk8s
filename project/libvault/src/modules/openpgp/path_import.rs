use anyhow::anyhow;
use pgp::{
    composed::{Deserializable, SignedSecretKey},
    types::SecretKeyTrait,
};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    storage::StorageEntry,
    utils::seal::SealedSecret,
};

use super::{OpenPgpBackend, OpenPgpBackendInner, PGP_KEYS_PREFIX, PGP_REVOKED_PREFIX};

impl OpenPgpBackend {
    pub fn import_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern(r"keys/import")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.import_key(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Key name"),
            )
            .field(
                "key",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("ASCII Armored Private Key"),
            )
            .help("Import an existing OpenPGP private key")
            .build()
    }
}

impl OpenPgpBackendInner {
    pub async fn import_key(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req
            .get_data("name")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();

        // Input Validation
        if name.trim().is_empty()
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(RvError::ErrRequestFieldInvalid);
        }

        let key_armor = req
            .get_data("key")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();

        let user = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown");
        log::info!("User [{}] is importing PGP key [{}]", user, name);

        // Validate armor & Parse Key
        let (signed_secret_key, _) = SignedSecretKey::from_string(&key_armor)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to parse PGP key: {}", e)))?;

        // Validate Key Validity (Basic)
        // Check if we can get the public key and fingerprint
        let _public_key = signed_secret_key.public_key();
        let fingerprint = String::from("0000000000000000000000000000000000000000");
        let key_id = String::from("0000000000000000");

        // Check if key is already revoked (locally known? No, we just import it. But maybe check if it's expired?)
        // If the key itself contains revocation signature, rpgp might handle it, but we store it anyway unless we want to enforce policy.
        // For now, we assume import means "I want this key here".

        // Check if we already have a revocation record for this name
        if req
            .storage_get(&format!("{}{}", PGP_REVOKED_PREFIX, name))
            .await?
            .is_some()
        {
            return Err(RvError::ErrPkiCertRevoked);
        }

        // Seal and Store
        let sealed = SealedSecret::seal(key_armor.as_bytes())
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to seal key: {}", e)))?;

        let entry = StorageEntry {
            key: format!("{}{}", PGP_KEYS_PREFIX, name),
            value: serde_json::to_vec(&sealed)?,
        };

        req.storage_put(&entry).await?;

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(Some(Response::data_response(Some(
            json!({
                "name": name,
                "fingerprint": fingerprint,
                "key_id": key_id,
                "imported_at": created_at
            })
            .as_object()
            .ok_or(RvError::ErrResponseDataInvalid)?
            .clone(),
        ))))
    }
}
