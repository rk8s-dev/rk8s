use anyhow::anyhow;
use pgp::{
    composed::{ArmorOptions, Message},
    crypto::hash::HashAlgorithm,
    types::PublicKeyTrait,
};
use rand::thread_rng;
use serde_json::json;

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
};

use super::{OpenPgpBackend, OpenPgpBackendInner};

impl OpenPgpBackend {
    pub fn sign_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern(r"keys/(?P<name>\w[\w-]+\w)/sign")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.sign_data(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Key name to sign with"),
            )
            .field(
                "input",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Data to sign (string or base64)"),
            )
            .help("Sign data with a stored OpenPGP key")
            .build()
    }
}

impl OpenPgpBackendInner {
    pub async fn sign_data(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_val = req.get_data("name")?;
        let name = name_val
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let data_val = req.get_data("input")?;
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
        log::info!("User [{}] is signing data with PGP key [{}]", user, name);

        let secret_key = self.get_private_key(req, &name).await?;

        let message = Message::new_literal_bytes("", data.as_bytes());

        let pass = passphrase.unwrap_or_default();

        let signed_message = message
            .sign(
                &mut thread_rng(),
                &secret_key,
                || pass.clone(),
                HashAlgorithm::SHA2_256,
            )
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to sign data: {}", e)))?;

        let signed_data = signed_message
            .to_armored_string(ArmorOptions::default())
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to armor signature: {}", e)))?;

        let fingerprint = hex::encode(secret_key.fingerprint().as_bytes());
        let key_id = hex::encode(secret_key.key_id().as_ref());

        let resp = json!({
            "signed_data": signed_data,
            "fingerprint": fingerprint,
            "key_id": key_id
        })
        .as_object()
        .ok_or(RvError::ErrResponseDataInvalid)?
        .clone();
        Ok(Some(Response::data_response(Some(resp))))
    }
}
