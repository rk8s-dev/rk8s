use pgp::composed::{Deserializable, DetachedSignature, SignedSecretKey};
use pgp::crypto::hash::HashAlgorithm;
use pgp::ser::Serialize;
use pgp::types::Password;
use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::SeedableRng;

use super::{PkiBackend, PkiBackendInner, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::{RequestExt, ResponseExt},
};

impl PkiBackend {
    pub fn pgp_sign_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern("pgp/sign")
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("PGP key name to sign with"),
            )
            .field(
                "data",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Hex-encoded data to sign"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.pgp_sign_data(backend, req).await })
                }
            })
            .help("Sign data with a PGP key.")
            .build()
    }
}

impl PkiBackendInner {
    pub async fn pgp_sign_data(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::PgpSignRequest = req.parse_json()?;

        let bundle = self.fetch_pgp_key(req, &payload.key_name).await?;

        let (secret_key, _) =
            SignedSecretKey::from_armor_single(bundle.armored_secret_key.as_bytes())
                .map_err(|_| RvError::ErrPkiPgpKeyNotFound)?;

        let data = hex::decode(payload.data.as_bytes())?;

        let mut rng = ChaCha20Rng::from_entropy();
        let signature = DetachedSignature::sign_binary_data(
            &mut rng,
            &secret_key.primary_key,
            &Password::empty(),
            HashAlgorithm::Sha256,
            &data[..],
        )
        .map_err(|_| RvError::ErrPkiInternal)?;

        let sig_bytes = signature.to_bytes().map_err(|_| RvError::ErrPkiInternal)?;

        let response = types::PgpSignResponse {
            signature: hex::encode(sig_bytes),
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }
}
