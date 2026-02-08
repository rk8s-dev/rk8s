use anyhow::anyhow;
use pgp::{Deserializable, composed::Message, types::SecretKeyTrait};
use serde_json::json;

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
};

use super::{OpenPgpBackend, OpenPgpBackendInner};

impl OpenPgpBackend {
    pub fn verify_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern(r"keys/(?P<name>\w[\w-]+\w)/verify")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.verify_data(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Key name to verify with"),
            )
            .field(
                "input",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Signed data/message"),
            )
            .field(
                "signature",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(false)
                    .description("Detached signature (optional if inline)"),
            )
            .help("Verify OpenPGP signature")
            .build()
    }
}

impl OpenPgpBackendInner {
    pub async fn verify_data(
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
        let signature_val = req
            .get_data("signature")
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        let user = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown");
        log::info!("User [{}] is verifying data with PGP key [{}]", user, name);

        let secret_key = self.get_private_key(req, &name).await?;
        let public_key = secret_key.public_key();

        // 1. Handle detached signature if present
        if let Some(sig_armor) = signature_val {
            // Verify detached signature
            // Use PacketParser since StandaloneSignature is missing or hard to find
            let parser = pgp::packet::PacketParser::new(std::io::Cursor::new(sig_armor.as_bytes()));

            let mut verified = false;
            for packet in parser {
                let packet: pgp::packet::Packet = packet.map_err(|e| {
                    RvError::ErrOther(anyhow!("Failed to parse signature packet: {}", e))
                })?;
                if let pgp::packet::Packet::Signature(sig) = packet {
                    sig.verify(&public_key, data.as_bytes()).map_err(|e| {
                        RvError::ErrOther(anyhow!("Detached signature verification failed: {}", e))
                    })?;
                    verified = true;
                }
            }

            if !verified {
                return Err(RvError::ErrOther(anyhow!(
                    "No valid signature found in provided armor"
                )));
            }

            return Ok(Some(Response::data_response(Some(
                json!({ "valid": true, "content": data })
                    .as_object()
                    .ok_or(RvError::ErrResponseDataInvalid)?
                    .clone(),
            ))));
        }

        // 2. Inline Signature Verification
        let (message, _) = Message::from_string(&data)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to parse message: {}", e)))?;

        let content_str: String = message
            .get_content()
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to get content: {}", e)))?
            .ok_or(RvError::ErrOther(anyhow!("No content")))?
            .iter()
            .map(|b| *b as char)
            .collect();

        message
            .verify(&public_key)
            .map_err(|e| RvError::ErrOther(anyhow!("Signature verification failed: {}", e)))?;

        Ok(Some(Response::data_response(Some(
            json!({ "valid": true, "content": content_str })
                .as_object()
                .ok_or(RvError::ErrResponseDataInvalid)?
                .clone(),
        ))))
    }
}
