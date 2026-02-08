use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    storage::StorageEntry,
};

use super::{SSH_REVOKED_PREFIX, SshBackend, SshBackendInner};

impl SshBackend {
    pub fn revoke_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("revoke")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.revoke_key(backend, req).await })
                }
            })
            .field(
                "key_id",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Key ID to revoke"),
            )
            .field(
                "serial",
                Field::builder()
                    .field_type(FieldType::Int)
                    .description("Serial number to revoke"),
            )
            .help("Revoke an SSH key by ID or Serial")
            .build()
    }

    pub fn revoked_list_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("config/revoked/?")
            .operation(Operation::List, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.list_revoked(backend, req).await })
                }
            })
            .help("List revoked SSH keys")
            .build()
    }
}

impl SshBackendInner {
    pub async fn revoke_key(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        // 1. Auth & Audit: Verify token and log user
        if req.client_token.is_empty() {
            return Err(RvError::ErrRequestClientTokenMissing);
        }
        let user = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown");
        log::info!("User [{}] is requesting SSH revocation", user);

        // 2. Parse Inputs
        let key_id = req
            .get_data("key_id")
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        // Strict Serial Parsing: intercept negative numbers
        let serial = if let Ok(val) = req.get_data("serial") {
            if let Some(n) = val.as_i64() {
                if n < 0 {
                    log::warn!("Attempted to revoke negative serial: {}", n);
                    return Err(RvError::ErrRequestFieldInvalid);
                }
                Some(n as u64)
            } else if let Some(n) = val.as_u64() {
                Some(n)
            } else {
                // Present but invalid format
                return Err(RvError::ErrRequestFieldInvalid);
            }
        } else {
            None
        };

        if key_id.is_none() && serial.is_none() {
            return Err(RvError::ErrRequestFieldInvalid);
        }

        // 3. Time Safety: Handle potential clock errors
        let revoked_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // 4. Execution with Idempotency Check
        if let Some(kid) = key_id {
            let key = format!("{SSH_REVOKED_PREFIX}id:{}", kid);
            // Check if already revoked to preserve original revocation time
            if req.storage_get(&key).await?.is_none() {
                let entry = StorageEntry {
                    key,
                    value: serde_json::to_vec(&json!({ "revoked_at": revoked_at }))?,
                };
                req.storage_put(&entry).await?;
                log::info!("Revoked SSH Key ID: {}", kid);
            } else {
                log::warn!("SSH Key ID {} already revoked, skipping overwrite.", kid);
            }
        }

        if let Some(ser) = serial {
            let key = format!("{SSH_REVOKED_PREFIX}serial:{}", ser);
            // Check if already revoked
            if req.storage_get(&key).await?.is_none() {
                let entry = StorageEntry {
                    key,
                    value: serde_json::to_vec(&json!({ "revoked_at": revoked_at }))?,
                };
                req.storage_put(&entry).await?;
                log::info!("Revoked SSH Serial: {}", ser);
            } else {
                log::warn!("SSH Serial {} already revoked, skipping overwrite.", ser);
            }
        }

        Ok(Some(Response::data_response(Some(
            json!({ "revoked": true })
                .as_object()
                .ok_or(RvError::ErrResponseDataInvalid)?
                .clone(),
        ))))
    }

    pub async fn list_revoked(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let keys = req.storage_list(SSH_REVOKED_PREFIX).await?;
        Ok(Some(Response::list_response(&keys)))
    }
}
