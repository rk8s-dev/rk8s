use anyhow::anyhow;
use serde_json::json;
use ssh_key::{Certificate, HashAlg, PrivateKey};

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    utils::seal::SealedSecret,
};

use super::{SSH_CA_PREFIX, SshBackend, SshBackendInner};

impl SshBackend {
    pub fn verify_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("verify")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.verify_cert(backend, req).await })
                }
            })
            .field(
                "certificate",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("SSH Certificate"),
            )
            .field(
                "ca_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(false)
                    .description("CA Name to verify against"),
            )
            .help("Verify an SSH certificate")
            .build()
    }
}

impl SshBackendInner {
    pub async fn verify_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let cert_str = req
            .get_data("certificate")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let ca_name = req
            .get_data("ca_name")
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        let cert = Certificate::from_openssh(&cert_str)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to parse certificate: {}", e)))?;

        let valid = if let Some(ca_name) = ca_name {
            let entry = req
                .storage_get(&format!("{SSH_CA_PREFIX}{}", ca_name))
                .await?;
            if let Some(entry) = entry {
                let ca_key_str =
                    if let Ok(secret) = serde_json::from_slice::<SealedSecret>(&entry.value) {
                        let key_bytes = secret.unwrap_key().map_err(|e| {
                            RvError::ErrOther(anyhow!("Failed to unwrap CA key: {}", e))
                        })?;
                        let mut sb = secret.box_data;
                        sb.unseal_with_key(&key_bytes).map_err(|e| {
                            RvError::ErrOther(anyhow!("Failed to unseal CA key: {}", e))
                        })?;
                        String::from_utf8(sb.get().map_err(|_| RvError::ErrPkiInternal)?.clone())?
                    } else {
                        String::from_utf8_lossy(&entry.value).to_string()
                    };

                let pk = PrivateKey::from_openssh(&ca_key_str)
                    .map_err(|e| RvError::ErrOther(anyhow!("Failed to load CA key: {}", e)))?;
                let ca_pub = pk.public_key();
                let fingerprint = ca_pub.fingerprint(HashAlg::Sha256);

                cert.validate(&[fingerprint]).is_ok()
            } else {
                false
            }
        } else {
            let entry = req.storage_get(&format!("{SSH_CA_PREFIX}default")).await?;
            if let Some(entry) = entry {
                let ca_key_str =
                    if let Ok(secret) = serde_json::from_slice::<SealedSecret>(&entry.value) {
                        let key_bytes = secret.unwrap_key().map_err(|e| {
                            RvError::ErrOther(anyhow!("Failed to unwrap CA key: {}", e))
                        })?;
                        let mut sb = secret.box_data;
                        sb.unseal_with_key(&key_bytes).map_err(|e| {
                            RvError::ErrOther(anyhow!("Failed to unseal CA key: {}", e))
                        })?;
                        String::from_utf8(sb.get().unwrap().clone()).unwrap()
                    } else {
                        String::from_utf8_lossy(&entry.value).to_string()
                    };

                let pk = PrivateKey::from_openssh(&ca_key_str)
                    .map_err(|e| RvError::ErrOther(anyhow!("Failed to load CA key: {}", e)))?;
                let ca_pub = pk.public_key();
                let fingerprint = ca_pub.fingerprint(HashAlg::Sha256);
                cert.validate(&[fingerprint]).is_ok()
            } else {
                false
            }
        };

        Ok(Some(Response::data_response(Some(
            json!({ "valid": valid }).as_object().unwrap().clone(),
        ))))
    }
}
