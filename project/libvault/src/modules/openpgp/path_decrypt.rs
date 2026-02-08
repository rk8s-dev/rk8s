use anyhow::anyhow;
use pgp::{Deserializable, composed::Message, errors::Error};
use serde_json::json;

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
};

use super::{OpenPgpBackend, OpenPgpBackendInner};

impl OpenPgpBackend {
    pub fn decrypt_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("pgp/decrypt")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.decrypt_data(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Key name to decrypt with"),
            )
            .field(
                "data",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Encrypted data (armored)"),
            )
            .help("Decrypt data using a stored OpenPGP key")
            .build()
    }
}

impl OpenPgpBackendInner {
    pub async fn decrypt_data(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_val = req.get_data("name")?;
        let name = name_val
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let data_val = req.get_data("data")?;
        let data = data_val
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let passphrase = req
            .get_data("passphrase")
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        let user = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown");
        log::info!("User [{}] is decrypting data with PGP key [{}]", user, name);

        let secret_key = self.get_private_key(req, &name).await?;

        let (message, _) = Message::from_string(&data)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to parse message: {}", e)))?;

        let pass = passphrase.unwrap_or_default();
        // Try to decrypt
        let (decrypted_message, _) =
            message
                .decrypt(|| pass.clone(), &[&secret_key])
                .map_err(|e: Error| {
                    // Check if error is related to passphrase
                    // The rpgp error type might be generic, but we can check the string
                    let err_msg = e.to_string();
                    if err_msg.to_lowercase().contains("password")
                        || err_msg.to_lowercase().contains("checksum")
                    {
                        RvError::ErrCredentailInvalid
                    } else {
                        RvError::ErrOther(anyhow!("Failed to decrypt message: {}", e))
                    }
                })?;

        let decrypted_message = decrypted_message;

        // Handle decompression
        let content_str: String = decrypted_message
            .get_content()
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to get content: {}", e)))?
            .ok_or(RvError::ErrOther(anyhow!("No content")))?
            .iter()
            .map(|b| *b as char)
            .collect();

        let resp = json!({ "decrypted_data": content_str })
            .as_object()
            .ok_or(RvError::ErrResponseDataInvalid)?
            .clone();
        Ok(Some(Response::data_response(Some(resp))))
    }
}
