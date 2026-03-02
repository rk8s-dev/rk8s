use humantime::parse_duration;
use openssl::{ec::EcKey, rsa::Rsa};
use pgp::composed::{
    ArmorOptions, Deserializable, DetachedSignature, EncryptionCaps, KeyType as PgpKeyType,
    SecretKeyParamsBuilder, SignedPublicKey, SignedSecretKey, SubkeyParamsBuilder,
};
use pgp::crypto::hash::HashAlgorithm;
use pgp::crypto::sym::SymmetricKeyAlgorithm;
use pgp::ser::Serialize as PgpSerialize;
use pgp::types::{CompressionAlgorithm, KeyDetails, Password};
use rand_chacha::ChaCha20Rng;
use rand_chacha::rand_core::SeedableRng;
use tracing::{info, warn};

use super::{CertBackend, PgpCertBackend, PkiBackend, PkiBackendInner, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::{RequestExt, ResponseExt},
    storage::StorageEntry,
    utils::key::{EncryptExtraData, KeyBundle},
};

const PKI_CONFIG_KEY_PREFIX: &str = "config/key/";

impl PkiBackend {
    pub fn keys_generate_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"keys/generate/(exported|internal)")
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("key name"),
            )
            .field(
                "key_bits",
                Field::builder()
                    .field_type(FieldType::Int)
                    .default_value(0)
                    .description("Key bits"),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("rsa")
                    .description("Key type: rsa, ec, pgp, aes-gcm, etc."),
            )
            // PGP-specific fields
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("User name for PGP key"),
            )
            .field(
                "email",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Email address for PGP key"),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("365d")
                    .description("Key expiration TTL (PGP)"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.generate_key(backend, req).await })
                }
            })
            .help("Generate a new key pair.")
            .build()
    }

    pub fn keys_import_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"keys/import")
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("key name"),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("rsa")
                    .description("The type of key to use; defaults to RSA. \"rsa\""),
            )
            .field(
                "pem_bundle",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("PEM-format, unencrypted secret"),
            )
            .field(
                "hex_bundle",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Hex-format, unencrypted secret"),
            )
            .field(
                "iv",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("IV for aes-gcm/aes-cbc"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.import_key(backend, req).await })
                }
            })
            .help("Import the specified key.")
            .build()
    }

    pub fn keys_sign_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"keys/sign")
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("key name"),
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
                    Box::pin(async move { handler.key_sign(backend, req).await })
                }
            })
            .help("Sign data (auto-detects PGP or generic key).")
            .build()
    }

    pub fn keys_verify_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"keys/verify")
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("key name"),
            )
            .field(
                "data",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Hex-encoded data to verify"),
            )
            .field(
                "signature",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Hex-encoded signature"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.key_verify(backend, req).await })
                }
            })
            .help("Verify a signature (auto-detects PGP or generic key).")
            .build()
    }

    pub fn keys_encrypt_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"keys/encrypt")
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("key name"),
            )
            .field(
                "data",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Hex-encoded data to encrypt"),
            )
            .field(
                "aad",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("")
                    .description("Additional Authenticated Data for aes-gcm/cbc"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.key_encrypt(backend, req).await })
                }
            })
            .help("Encrypt data.")
            .build()
    }

    pub fn keys_decrypt_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"keys/decrypt")
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("key name"),
            )
            .field(
                "data",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Hex-encoded data to decrypt"),
            )
            .field(
                "aad",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("")
                    .description("Additional Authenticated Data for aes-gcm/cbc"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.key_decrypt(backend, req).await })
                }
            })
            .help("Decrypt data.")
            .build()
    }
}

impl PkiBackendInner {
    // ── Key generation (with PGP branch) ──

    pub async fn generate_key(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::KeyGenerateRequest = req.parse_json()?;
        let key_name = payload.key_name;
        let key_type = payload
            .key_type
            .unwrap_or_else(|| "rsa".to_string())
            .to_lowercase();
        let key_bits = payload.key_bits.unwrap_or(0);

        // PGP branch
        if key_type == "pgp" {
            return self.pgp_generate_key(req, &key_name, key_bits).await;
        }

        let mut export_private_key = false;
        if req.path.ends_with("/exported") {
            export_private_key = true;
        }

        let key_info = self.fetch_key(req, &key_name).await;
        if key_info.is_ok() {
            return Err(RvError::ErrPkiKeyNameAlreadyExist);
        }

        let mut key_bundle = KeyBundle::new(&key_name, &key_type, key_bits);
        key_bundle.generate()?;

        self.write_key(req, &key_bundle).await?;

        let mut response = types::KeyOperationResponse {
            key_id: key_bundle.id.clone(),
            key_name: key_bundle.name.clone(),
            key_type: key_bundle.key_type.clone(),
            key_bits: key_bundle.bits,
            private_key: None,
            iv: None,
        };

        if export_private_key {
            match key_type.as_str() {
                "rsa" | "ec" | "sm2" => {
                    response.private_key =
                        Some(String::from_utf8_lossy(&key_bundle.key).to_string());
                }
                _ => {
                    response.private_key = Some(hex::encode(&key_bundle.key));
                }
            }

            if !key_bundle.iv.is_empty() {
                response.iv = Some(hex::encode(&key_bundle.iv));
            }
        }

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    // ── PGP key generation ──

    async fn pgp_generate_key(
        &self,
        req: &mut Request,
        key_name: &str,
        key_bits: u32,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::PgpGenerateRequest = req.parse_json()?;

        let mut export_private = false;
        if req.path.ends_with("/exported") {
            export_private = true;
        }

        if PgpCertBackend.fetch_cert(req, key_name).await.is_ok() {
            return Err(RvError::ErrPkiPgpKeyNameAlreadyExist);
        }

        let key_type_str = payload
            .key_type
            .filter(|t| t != "pgp")
            .unwrap_or_else(|| "rsa".to_string());
        let pgp_key_bits = if key_bits > 0 {
            key_bits
        } else {
            payload.key_bits.unwrap_or(2048)
        };
        let ttl_str = payload.ttl.unwrap_or_else(|| "365d".to_string());
        let _ttl = parse_duration(&ttl_str)?;
        warn!(ttl = %ttl_str, "PGP key expiration is not yet enforced at the OpenPGP layer");

        if key_type_str == "rsa" && !(2048..=8192).contains(&pgp_key_bits) {
            return Err(RvError::ErrPkiKeyBitsInvalid);
        }
        // PLACEHOLDER_PGP_GEN

        let primary_key_type = match key_type_str.as_str() {
            "rsa" => PgpKeyType::Rsa(pgp_key_bits),
            "ed25519" => PgpKeyType::Ed25519,
            _ => return Err(RvError::ErrPkiKeyTypeInvalid),
        };

        let subkey_type = match key_type_str.as_str() {
            "rsa" => PgpKeyType::Rsa(pgp_key_bits),
            "ed25519" => PgpKeyType::X25519,
            _ => return Err(RvError::ErrPkiKeyTypeInvalid),
        };

        if !payload.email.contains('@')
            || payload.email.contains('<')
            || payload.email.contains('>')
        {
            return Err(RvError::ErrRequestFieldInvalid);
        }

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
            key_name: key_name.to_string(),
            name: payload.name.clone(),
            email: payload.email.clone(),
            armored_secret_key: armored_secret.clone(),
            armored_public_key: armored_public.clone(),
            fingerprint: fingerprint.clone(),
            key_id: key_id_hex.clone(),
        };

        PgpCertBackend.store_cert(req, key_name, &bundle).await?;

        info!(
            key_name = %key_name,
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

    // ── Import ──

    pub async fn import_key(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::KeyImportRequest = req.parse_json()?;
        let key_name = payload.key_name;
        let key_type = payload
            .key_type
            .unwrap_or_else(|| "rsa".to_string())
            .to_lowercase();
        let pem_bundle = payload.pem_bundle.unwrap_or_default();
        let hex_bundle = payload.hex_bundle.unwrap_or_default();

        if pem_bundle.is_empty() && hex_bundle.is_empty() {
            return Err(RvError::ErrRequestFieldNotFound);
        }

        let key_info = self.fetch_key(req, &key_name).await;
        if key_info.is_ok() {
            return Err(RvError::ErrPkiKeyNameAlreadyExist);
        }

        let mut key_bundle = KeyBundle::new(&key_name, &key_type, 0);

        if !pem_bundle.is_empty() {
            key_bundle.key = pem_bundle.as_bytes().to_vec();
            match key_type.as_str() {
                "rsa" => {
                    let rsa = Rsa::private_key_from_pem(&key_bundle.key)?;
                    key_bundle.bits = rsa.size() * 8;
                }
                "ec" | "sm2" => {
                    let ec_key = EcKey::private_key_from_pem(&key_bundle.key)?;
                    key_bundle.bits = ec_key.group().degree();
                }
                _ => {
                    return Err(RvError::ErrPkiKeyTypeInvalid);
                }
            };
        }

        if !hex_bundle.is_empty() {
            key_bundle.key = hex::decode(hex_bundle)?;
            key_bundle.bits = (key_bundle.key.len() as u32) * 8;
            match key_bundle.bits {
                128 | 192 | 256 => {}
                _ => return Err(RvError::ErrPkiKeyBitsInvalid),
            };

            let is_iv_required = matches!(
                key_type.as_str(),
                "aes-gcm" | "aes-cbc" | "sm4-gcm" | "sm4-ccm"
            );
            #[cfg(feature = "crypto_adaptor_tongsuo")]
            let is_valid_key_type = matches!(
                key_type.as_str(),
                "aes-gcm" | "aes-cbc" | "aes-ecb" | "sm4-gcm" | "sm4-ccm"
            );
            #[cfg(not(feature = "crypto_adaptor_tongsuo"))]
            let is_valid_key_type = matches!(key_type.as_str(), "aes-gcm" | "aes-cbc" | "aes-ecb");

            if !is_valid_key_type {
                return Err(RvError::ErrPkiKeyTypeInvalid);
            }

            if is_iv_required {
                if let Some(iv) = payload.iv.as_deref() {
                    if iv.is_empty() {
                        return Err(RvError::ErrRequestFieldInvalid);
                    }
                    key_bundle.iv = hex::decode(iv)?;
                } else {
                    return Err(RvError::ErrRequestFieldNotFound);
                }
            }
        }

        self.write_key(req, &key_bundle).await?;

        let response = types::KeyOperationResponse {
            key_id: key_bundle.id.clone(),
            key_name: key_bundle.name.clone(),
            key_type: key_bundle.key_type.clone(),
            key_bits: key_bundle.bits,
            private_key: None,
            iv: None,
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    // ── Sign (auto-detect PGP) ──

    pub async fn key_sign(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::KeySignRequest = req.parse_json()?;

        // Try PGP first
        if let Ok(bundle) = self.fetch_pgp_key(req, &payload.key_name).await {
            return self.pgp_sign_with_bundle(&bundle, &payload.data).await;
        }

        // Fall back to generic KeyBundle
        let key_bundle = self.fetch_key(req, &payload.key_name).await?;
        let decoded_data = hex::decode(payload.data.as_bytes())?;
        let result = key_bundle.sign(&decoded_data)?;

        let response = types::KeyHexResult {
            result: hex::encode(result),
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    // ── Verify (auto-detect PGP) ──

    pub async fn key_verify(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::KeyVerifyRequest = req.parse_json()?;

        // Try PGP first
        if let Ok(bundle) = self.fetch_pgp_key(req, &payload.key_name).await {
            return self
                .pgp_verify_with_bundle(&bundle, &payload.data, &payload.signature)
                .await;
        }

        // Fall back to generic KeyBundle
        let key_bundle = self.fetch_key(req, &payload.key_name).await?;
        let decoded_data = hex::decode(payload.data.as_bytes())?;
        let decoded_signature = hex::decode(payload.signature.as_bytes())?;
        let result = key_bundle.verify(&decoded_data, &decoded_signature)?;

        let response = types::KeyVerifyResult { result };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    // ── Encrypt / Decrypt ──

    pub async fn key_encrypt(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::KeyCryptRequest = req.parse_json()?;
        let key_bundle = self.fetch_key(req, &payload.key_name).await?;

        let decoded_data = hex::decode(payload.data.as_bytes())?;
        let aad = payload.aad.unwrap_or_default();
        let result =
            key_bundle.encrypt(&decoded_data, Some(EncryptExtraData::Aad(aad.as_bytes())))?;

        let response = types::KeyHexResult {
            result: hex::encode(result),
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    pub async fn key_decrypt(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::KeyCryptRequest = req.parse_json()?;
        let key_bundle = self.fetch_key(req, &payload.key_name).await?;

        let decoded_data = hex::decode(payload.data.as_bytes())?;
        let aad = payload.aad.unwrap_or_default();
        let result =
            key_bundle.decrypt(&decoded_data, Some(EncryptExtraData::Aad(aad.as_bytes())))?;

        let response = types::KeyHexResult {
            result: hex::encode(result),
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    // ── PGP sign/verify helpers ──

    async fn pgp_sign_with_bundle(
        &self,
        bundle: &types::PgpKeyBundle,
        data_hex: &str,
    ) -> Result<Option<Response>, RvError> {
        let (secret_key, _) =
            SignedSecretKey::from_armor_single(bundle.armored_secret_key.as_bytes())
                .map_err(|_| RvError::ErrPkiPgpKeyNotFound)?;

        let data = hex::decode(data_hex.as_bytes())?;

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

    async fn pgp_verify_with_bundle(
        &self,
        bundle: &types::PgpKeyBundle,
        data_hex: &str,
        signature_hex: &str,
    ) -> Result<Option<Response>, RvError> {
        let (public_key, _) =
            SignedPublicKey::from_armor_single(bundle.armored_public_key.as_bytes())
                .map_err(|_| RvError::ErrPkiPgpKeyNotFound)?;

        let data = hex::decode(data_hex.as_bytes())?;
        let sig_raw = hex::decode(signature_hex.as_bytes())?;

        let sig =
            DetachedSignature::from_bytes(&sig_raw[..]).map_err(|_| RvError::ErrPkiDataInvalid)?;

        let valid = sig.verify(&public_key, &data).is_ok();

        let response = types::PgpVerifyResult { valid };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    // ── Storage helpers ──

    pub async fn fetch_key(&self, req: &Request, key_name: &str) -> Result<KeyBundle, RvError> {
        let entry = req
            .storage_get(format!("{PKI_CONFIG_KEY_PREFIX}{key_name}").as_str())
            .await?;
        if entry.is_none() {
            return Err(RvError::ErrPkiCertNotFound);
        }

        let key_bundle: KeyBundle = serde_json::from_slice(entry.unwrap().value.as_slice())?;
        Ok(key_bundle)
    }

    pub async fn write_key(&self, req: &Request, key_bundle: &KeyBundle) -> Result<(), RvError> {
        let key_name = format!("{}{}", PKI_CONFIG_KEY_PREFIX, key_bundle.name);
        let entry = StorageEntry::new(key_name.as_str(), key_bundle)?;
        req.storage_put(&entry).await?;
        Ok(())
    }
}
