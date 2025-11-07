use openssl::{ec::EcKey, rsa::Rsa};

use super::{PkiBackend, PkiBackendInner, types};
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
                    .description(
                        r#"
The number of bits to use. Allowed values are 0 (universal default); with rsa
key_type: 2048 (default), 3072, or 4096; with ec key_type: 224, 256 (default),
384, or 521; ignored with ed25519."#,
                    ),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("rsa")
                    .description("The type of key to use; defaults to RSA. \"rsa\""),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.generate_key(backend, req).await })
                }
            })
            .help(
                r#"
This endpoint will generate a new key pair of the specified type (internal, exported)
used for sign,verify,encrypt,decrypt.
                "#,
            )
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
                    .description("Data that needs to be signed"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.key_sign(backend, req).await })
                }
            })
            .help("Data Signatures.")
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
                    .description("Data that needs to be verified"),
            )
            .field(
                "signature",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Signature data"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.key_verify(backend, req).await })
                }
            })
            .help("Data verification.")
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
                    .description("Data that needs to be encrypted"),
            )
            .field(
                "aad",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("")
                    .description(
                        "Additional Authenticated Data can be provided for aes-gcm/cbc encryption",
                    ),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.key_encrypt(backend, req).await })
                }
            })
            .help("Data encryption.")
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
                    .description("Data that needs to be decrypted"),
            )
            .field(
                "aad",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("")
                    .description(
                        "Additional Authenticated Data can be provided for aes-gcm/cbc decryption",
                    ),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.key_decrypt(backend, req).await })
                }
            })
            .help("Data decryption.")
            .build()
    }
}

impl PkiBackendInner {
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

    pub async fn key_sign(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::KeySignRequest = req.parse_json()?;

        let key_bundle = self.fetch_key(req, &payload.key_name).await?;

        let decoded_data = hex::decode(payload.data.as_bytes())?;
        let result = key_bundle.sign(&decoded_data)?;

        let response = types::KeyHexResult {
            result: hex::encode(result),
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    pub async fn key_verify(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::KeyVerifyRequest = req.parse_json()?;

        let key_bundle = self.fetch_key(req, &payload.key_name).await?;

        let decoded_data = hex::decode(payload.data.as_bytes())?;
        let decoded_signature = hex::decode(payload.signature.as_bytes())?;
        let result = key_bundle.verify(&decoded_data, &decoded_signature)?;

        let response = types::KeyVerifyResult { result };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

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
