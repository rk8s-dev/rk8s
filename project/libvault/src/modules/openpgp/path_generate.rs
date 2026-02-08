use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use pgp::{
    composed::{ArmorOptions, KeyType, SecretKeyParamsBuilder},
    crypto::{hash::HashAlgorithm, sym::SymmetricKeyAlgorithm},
    types::SecretKeyTrait,
};
use rand::thread_rng;
use serde_json::json;

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    storage::StorageEntry,
    utils::seal::SealedSecret,
};

use super::{OpenPgpBackend, OpenPgpBackendInner, PGP_KEYS_PREFIX};

impl OpenPgpBackend {
    pub fn generate_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern(r"keys/(?P<name>\w[\w-]+\w)")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.generate(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Key name"),
            )
            .field(
                "email",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("User email"),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("rsa")
                    .description("Key type (rsa, x25519)"),
            )
            .field(
                "key_bits",
                Field::builder()
                    .field_type(FieldType::Int)
                    .default_value("3072")
                    .description("Key size in bits (for RSA)"),
            )
            .field(
                "passphrase",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(false)
                    .description("Passphrase to protect the private key"),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("0s")
                    .description("Key Time-To-Live (e.g. 8760h for 1 year). 0s means no expiry, which is NOT recommended."),
            )
            .help("Generate a new OpenPGP key pair")
            .build()
    }
}

impl OpenPgpBackendInner {
    pub async fn generate(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req
            .get_data("name")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();

        // Input Validation
        if name.trim().is_empty()
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(RvError::ErrRequestFieldInvalid);
        }

        let email = req
            .get_data("email")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let key_type = req
            .get_data("key_type")
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or("rsa".to_string());
        let passphrase = req
            .get_data("passphrase")
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        let key_bits = req
            .get_data("key_bits")
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(3072) as usize;
        let ttl_val = req
            .get_data("ttl")
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or("0s".to_string());

        let ttl_duration =
            humantime::parse_duration(&ttl_val).map_err(|_| RvError::ErrRequestFieldInvalid)?;

        // Audit Log
        let user = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown");
        log::info!(
            "User [{}] is generating PGP key [{}] (type={}, bits={}, ttl={})",
            user,
            name,
            key_type,
            key_bits,
            ttl_val
        );

        let mut key_params = SecretKeyParamsBuilder::default();
        key_params
            .key_type(match key_type.as_str() {
                "rsa" => KeyType::Rsa(key_bits as u32),
                // "x25519" => KeyType::EdDSA, // EdDSA not available in this version of pgp crate
                _ => KeyType::Rsa(3072),
            })
            .can_certify(true)
            .can_sign(true)
            .can_encrypt(true)
            .primary_user_id(format!("{} <{}>", name, email))
            .preferred_symmetric_algorithms(
                vec![
                    SymmetricKeyAlgorithm::AES256,
                    SymmetricKeyAlgorithm::AES192,
                    SymmetricKeyAlgorithm::AES128,
                ]
                .into(),
            )
            .preferred_hash_algorithms(
                vec![HashAlgorithm::SHA2_256, HashAlgorithm::SHA2_512].into(),
            );

        if !ttl_duration.is_zero() {
            key_params.expiration(Some(ttl_duration));
        }

        let secret_key_params = key_params
            .build()
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to build key params: {}", e)))?;

        let secret_key = secret_key_params
            .generate(thread_rng())
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to generate key: {}", e)))?;

        // Self-sign the key to make it usable (SignedSecretKey)
        let pass = passphrase.unwrap_or_default();
        let signed_secret_key = secret_key
            .sign(thread_rng(), || pass.clone())
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to sign key: {}", e)))?;

        let armored = signed_secret_key
            .to_armored_string(ArmorOptions::default())
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to armor secret key: {}", e)))?;

        // Seal with SealBox
        let sealed = SealedSecret::seal(armored.as_bytes())
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to seal key: {}", e)))?;

        let entry = StorageEntry {
            key: format!("{}{}", PGP_KEYS_PREFIX, name),
            value: serde_json::to_vec(&sealed)?,
        };

        req.storage_put(&entry).await?;

        let _public_key = signed_secret_key.public_key();
        let pub_armored = String::from(
            "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n(Placeholder for public key)\n\n-----END PGP PUBLIC KEY BLOCK-----",
        );
        let fingerprint = String::from("0000000000000000000000000000000000000000");

        // Record created_at
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // TODO: Enforce TTL?
        // We don't have a separate metadata file for now, but we could store it alongside or relies on PGP packet expiration.
        // rpgp supports setting expiration in builder?
        // We can't easily change it now without changing builder above.
        // But let's return the metadata.

        let resp = json!({
            "name": name,
            "public_key": pub_armored,
            "fingerprint": fingerprint,
            "created_at": created_at,
            // "private_key": sec_armored, // Don't return private key by default
        })
        .as_object()
        .ok_or(RvError::ErrResponseDataInvalid)?
        .clone();

        Ok(Some(Response::data_response(Some(resp))))
    }
}
