use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};

use super::{PkiBackend, PkiBackendInner, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::{RequestExt, ResponseExt},
};

impl PkiBackend {
    pub fn pgp_verify_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern("pgp/verify")
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("PGP key name to verify with"),
            )
            .field(
                "data",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Hex-encoded data that was signed"),
            )
            .field(
                "signature",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Hex-encoded signature to verify"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.pgp_verify_data(backend, req).await })
                }
            })
            .help("Verify a PGP signature.")
            .build()
    }
}

impl PkiBackendInner {
    pub async fn pgp_verify_data(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::PgpVerifyRequest = req.parse_json()?;

        let bundle = self.fetch_pgp_key(req, &payload.key_name).await?;

        let (public_key, _) =
            SignedPublicKey::from_armor_single(bundle.armored_public_key.as_bytes())
                .map_err(|_| RvError::ErrPkiPgpKeyNotFound)?;

        let data = hex::decode(payload.data.as_bytes())?;
        let sig_raw = hex::decode(payload.signature.as_bytes())?;

        let sig =
            DetachedSignature::from_bytes(&sig_raw[..]).map_err(|_| RvError::ErrPkiDataInvalid)?;

        let valid = sig.verify(&public_key, &data).is_ok();

        let response = types::PgpVerifyResult { valid };

        Ok(Some(Response::data_response(response.to_map()?)))
    }
}
