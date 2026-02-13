use humantime::parse_duration;
use pgp::composed::{
    ArmorOptions, EncryptionCaps, KeyType as PgpKeyType, SecretKeyParamsBuilder,
    SubkeyParamsBuilder,
};
use pgp::crypto::hash::HashAlgorithm;
use pgp::crypto::sym::SymmetricKeyAlgorithm;
use pgp::types::{CompressionAlgorithm, KeyDetails};
use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::SeedableRng;
use tracing::info;

use super::{PkiBackend, PkiBackendInner, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::{RequestExt, ResponseExt},
    storage::StorageEntry,
};

const PGP_KEY_PREFIX: &str = "pgp/keys/";

impl PkiBackend {
    pub fn pgp_generate_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"pgp/generate/(exported|internal)")
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("User name for the PGP key"),
            )
            .field(
                "email",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Email address for the PGP key"),
            )
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Storage key name"),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("rsa")
                    .description("Key type: rsa, ed25519"),
            )
            .field(
                "key_bits",
                Field::builder()
                    .field_type(FieldType::Int)
                    .default_value(2048)
                    .description("Key bits (for RSA)"),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("365d")
                    .description("Key expiration TTL"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.pgp_generate_key(backend, req).await })
                }
            })
            .help("Generate a new PGP key pair.")
            .build()
    }
}

impl PkiBackendInner {
    pub async fn pgp_generate_key(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::PgpGenerateRequest = req.parse_json()?;

        let mut export_private = false;
        if req.path.ends_with("/exported") {
            export_private = true;
        }

        // Check if key_name already exists
        let existing = req
            .storage_get(format!("{PGP_KEY_PREFIX}{}", payload.key_name).as_str())
            .await?;
        if existing.is_some() {
            return Err(RvError::ErrPkiPgpKeyNameAlreadyExist);
        }

        let key_type_str = payload.key_type.unwrap_or_else(|| "rsa".to_string());
        let key_bits = payload.key_bits.unwrap_or(2048);
        let ttl_str = payload.ttl.unwrap_or_else(|| "365d".to_string());
        let _ttl = parse_duration(&ttl_str)?;

        // Validate RSA key strength
        if key_type_str == "rsa" && !(2048..=8192).contains(&key_bits) {
            return Err(RvError::ErrPkiKeyBitsInvalid);
        }

        let primary_key_type = match key_type_str.as_str() {
            "rsa" => PgpKeyType::Rsa(key_bits),
            "ed25519" => PgpKeyType::Ed25519,
            _ => return Err(RvError::ErrPkiKeyTypeInvalid),
        };

        let subkey_type = match key_type_str.as_str() {
            "rsa" => PgpKeyType::Rsa(key_bits),
            "ed25519" => PgpKeyType::X25519,
            _ => return Err(RvError::ErrPkiKeyTypeInvalid),
        };

        let user_id = format!("{} <{}>", payload.name, payload.email);

        let subkey = SubkeyParamsBuilder::default()
            .key_type(subkey_type)
            .can_encrypt(EncryptionCaps::All)
            .build()
            .map_err(|_| RvError::ErrPkiPgpKeyGenerationFailed)?;

        let key_params = SecretKeyParamsBuilder::default()
            .key_type(primary_key_type)
            .can_certify(true)
            .can_sign(true)
            .primary_user_id(user_id)
            .preferred_symmetric_algorithms(smallvec::smallvec![
                SymmetricKeyAlgorithm::AES256,
                SymmetricKeyAlgorithm::AES192,
                SymmetricKeyAlgorithm::AES128,
            ])
            .preferred_hash_algorithms(smallvec::smallvec![
                HashAlgorithm::Sha256,
                HashAlgorithm::Sha384,
                HashAlgorithm::Sha512,
                HashAlgorithm::Sha224,
                HashAlgorithm::Sha1,
            ])
            .preferred_compression_algorithms(smallvec::smallvec![
                CompressionAlgorithm::ZLIB,
                CompressionAlgorithm::ZIP,
            ])
            .subkeys(vec![subkey])
            .build()
            .map_err(|_| RvError::ErrPkiPgpKeyGenerationFailed)?;

        let (armored_public, armored_secret, fingerprint, key_id_hex) = {
            let mut rng = ChaCha20Rng::from_entropy();
            let signed_secret_key = key_params
                .generate(&mut rng)
                .map_err(|_| RvError::ErrPkiPgpKeyGenerationFailed)?;

            let signed_public_key = signed_secret_key.to_public_key();

            let armored_public = signed_public_key
                .to_armored_string(ArmorOptions::default())
                .map_err(|_| RvError::ErrPkiPgpKeyGenerationFailed)?;
            let armored_secret = signed_secret_key
                .to_armored_string(ArmorOptions::default())
                .map_err(|_| RvError::ErrPkiPgpKeyGenerationFailed)?;

            let fingerprint = hex::encode(signed_public_key.fingerprint().as_bytes());
            let key_id_hex = hex::encode(signed_public_key.legacy_key_id().as_ref()).to_uppercase();

            (armored_public, armored_secret, fingerprint, key_id_hex)
        };

        let bundle = types::PgpKeyBundle {
            key_name: payload.key_name.clone(),
            name: payload.name.clone(),
            email: payload.email.clone(),
            armored_secret_key: armored_secret.clone(),
            armored_public_key: armored_public.clone(),
            fingerprint: fingerprint.clone(),
            key_id: key_id_hex.clone(),
        };

        let entry = StorageEntry::new(
            format!("{PGP_KEY_PREFIX}{}", payload.key_name).as_str(),
            &bundle,
        )?;
        req.storage_put(&entry).await?;

        info!(
            key_name = %payload.key_name,
            key_type = %key_type_str,
            fingerprint = %fingerprint,
            key_id = %key_id_hex,
            exported = export_private,
            "PGP key generated"
        );

        let response = types::PgpGenerateResponse {
            public_key: armored_public,
            private_key: if export_private {
                Some(armored_secret)
            } else {
                None
            },
            fingerprint,
            key_id: key_id_hex,
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    pub async fn fetch_pgp_key(
        &self,
        req: &Request,
        key_name: &str,
    ) -> Result<types::PgpKeyBundle, RvError> {
        let entry = req
            .storage_get(format!("{PGP_KEY_PREFIX}{key_name}").as_str())
            .await?;
        if entry.is_none() {
            return Err(RvError::ErrPkiPgpKeyNotFound);
        }
        let bundle: types::PgpKeyBundle = serde_json::from_slice(entry.unwrap().value.as_slice())?;
        Ok(bundle)
    }
}
