use anyhow::anyhow;
// use pgp::{
//     composed::{ArmorOptions, SignedSecretKey},
//     crypto::sym::SymmetricKeyAlgorithm,
// };
// use rand::thread_rng;
// use serde_json::json;

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
};

use super::{OpenPgpBackend, OpenPgpBackendInner};

impl OpenPgpBackend {
    pub fn encrypt_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("pgp/encrypt")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.encrypt_data(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Key name to encrypt for"),
            )
            .field(
                "data",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Data to encrypt"),
            )
            .help("Encrypt data for a stored OpenPGP key")
            .build()
    }
}

impl OpenPgpBackendInner {
    pub async fn encrypt_data(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        // Temporary disabled due to API changes in pgp crate
        Err(RvError::ErrOther(anyhow!("Encryption not implemented yet")))
    }
}
