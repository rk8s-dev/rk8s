use pgp::{
    composed::SignedSecretKey,
    types::{PublicKeyTrait, SecretKeyTrait},
};
use serde_json::json;

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
};

use super::{OpenPgpBackend, OpenPgpBackendInner};

impl OpenPgpBackend {
    pub fn fetch_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern(r"pgp/fetch/(?P<name>\w[\w-]+\w)")
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.fetch(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Key name"),
            )
            .help("Fetch stored OpenPGP keys (armored)")
            .build()
    }

    pub fn export_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern(r"pgp/export/(?P<name>\w[\w-]+\w)")
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.export_key(backend, req).await })
                }
            })
            .help("Export OpenPGP public key")
            .build()
    }
}

impl OpenPgpBackendInner {
    pub async fn fetch(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_val = req.get_data("name")?;
        let name = name_val.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        let user = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown");
        log::info!("User [{}] is fetching PGP key [{}]", user, name);

        let secret_key: SignedSecretKey = self.get_private_key(req, name).await?;
        let public_key = secret_key.public_key();
        let pub_armored = String::from(
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n(Placeholder for public key)\n\n-----END PGP PUBLIC KEY BLOCK-----",
        );
        let fingerprint = String::from("0000000000000000000000000000000000000000");
        let key_id = String::from("0000000000000000");

        let created_at_str = public_key.created_at().to_rfc3339();

        let resp = json!({
            "public_key": pub_armored,
            "fingerprint": fingerprint,
            "key_id": key_id,
            "created_at": created_at_str,
        })
        .as_object()
        .ok_or(RvError::ErrResponseDataInvalid)?
        .clone();
        Ok(Some(Response::data_response(Some(resp))))
    }

    pub async fn export_key(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_val = req.get_data("name")?;
        let name = name_val
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();

        let user = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown");
        log::info!("User [{}] is exporting PGP public key [{}]", user, name);

        let secret_key: SignedSecretKey = self.get_private_key(req, &name).await?;
        let _public_key = secret_key.public_key();
        let pub_armored = String::from(
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n(Placeholder for public key)\n\n-----END PGP PUBLIC KEY BLOCK-----",
        );

        let resp = json!({ "public_key": pub_armored })
            .as_object()
            .ok_or(RvError::ErrResponseDataInvalid)?
            .clone();
        Ok(Some(Response::data_response(Some(resp))))
    }
}
