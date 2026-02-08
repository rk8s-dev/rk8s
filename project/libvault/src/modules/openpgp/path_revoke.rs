use anyhow::anyhow;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    storage::StorageEntry,
};

use super::{OpenPgpBackend, OpenPgpBackendInner};

pub const PGP_REVOKED_PREFIX: &str = "pgp/revoked/";

impl OpenPgpBackend {
    pub fn revoke_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern(r"keys/(?P<name>\w[\w-]+\w)/revoke")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.revoke_key(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Key name to revoke"),
            )
            .field(
                "reason",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(false)
                    .default_value("Key is superseded")
                    .description("Revocation reason"),
            )
            .help("Revoke an OpenPGP key (Logical Revocation)")
            .build()
    }
}

impl OpenPgpBackendInner {
    pub async fn revoke_key(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_val = req.get_data("name")?;
        let name = name_val
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();

        let reason = if let Ok(val) = req.get_data("reason") {
            val.as_str().unwrap_or("Key is superseded").to_string()
        } else {
            "Key is superseded".to_string()
        };

        // 1. Audit Logging
        // Clone user string to avoid borrowing req
        let user = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown")
            .to_string();
        log::info!("User [{}] is revoking PGP key [{}]", user, name);

        // 2. Ensure Key Exists
        // Pass req mutably
        let _ = self.get_private_key(req, &name).await?;

        // 3. Perform Logical Revocation
        let revoked_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let revocation_entry = StorageEntry {
            key: format!("{}{}", PGP_REVOKED_PREFIX, name),
            value: serde_json::to_vec(&json!({
                "revoked_at": revoked_at,
                "reason": reason,
                "revoked_by": user
            }))
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to serialize revocation: {}", e)))?,
        };

        req.storage_put(&revocation_entry).await?;

        Ok(Some(Response::data_response(Some(
            json!({
                "name": name,
                "revoked": true,
                "revoked_at": revoked_at
            })
            .as_object()
            .ok_or(RvError::ErrResponseDataInvalid)?
            .clone(),
        ))))
    }
}
