use std::{collections::HashMap, sync::Arc};

use openssl::{ec::EcKey, rsa::Rsa};
use serde_json::{json, Value};

use super::{PkiBackend, PkiBackendInner};
use crate::{
    context::Context,
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, PathOperation, Request, Response},
    new_fields, new_fields_internal, new_path, new_path_internal,
    storage::StorageEntry,
    utils::key::{EncryptExtraData, KeyBundle},
};

const PKI_CONFIG_KEY_PREFIX: &str = "config/key/";

impl PkiBackend {
    pub fn keys_generate_path(&self) -> Path {
        let pki_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"keys/generate/(exported|internal)",
            fields: {
                "key_name": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "key name"
                },
                "key_bits": {
                    field_type: FieldType::Int,
                    default: 0,
                    description: r#"
The number of bits to use. Allowed values are 0 (universal default); with rsa
key_type: 2048 (default), 3072, or 4096; with ec key_type: 224, 256 (default),
384, or 521; ignored with ed25519."#
                },
                "key_type": {
                    field_type: FieldType::Str,
                    default: "rsa",
                    description: r#"The type of key to use; defaults to RSA. "rsa""#
                }
            },
            operations: [
                {op: Operation::Write, handler: pki_backend_ref.generate_key}
            ],
            help: r#"
This endpoint will generate a new key pair of the specified type (internal, exported)
used for sign,verify,encrypt,decrypt.
                "#
        });

        path
    }

    pub fn keys_import_path(&self) -> Path {
        let pki_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"keys/import",
            fields: {
                "key_name": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "key name"
                },
                "key_type": {
                    field_type: FieldType::Str,
                    default: "rsa",
                    description: r#"The type of key to use; defaults to RSA. "rsa""#
                },
                "pem_bundle": {
                    field_type: FieldType::Str,
                    description: "PEM-format, unencrypted secret"
                },
                "hex_bundle": {
                    field_type: FieldType::Str,
                    description: "Hex-format, unencrypted secret"
                },
                "iv": {
                    field_type: FieldType::Str,
                    description: "IV for aes-gcm/aes-cbc"
                }
            },
            operations: [
                {op: Operation::Write, handler: pki_backend_ref.import_key}
            ],
            help: "Import the specified key."
        });

        path
    }

    pub fn keys_sign_path(&self) -> Path {
        let pki_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"keys/sign",
            fields: {
                "key_name": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "key name"
                },
                "data": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "Data that needs to be signed"
                }
            },
            operations: [
                {op: Operation::Write, handler: pki_backend_ref.key_sign}
            ],
            help: "Data Signatures."
        });

        path
    }

    pub fn keys_verify_path(&self) -> Path {
        let pki_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"keys/verify",
            fields: {
                "key_name": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "key name"
                },
                "data": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "Data that needs to be verified"
                },
                "signature": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "Signature data"
                }
            },
            operations: [
                {op: Operation::Write, handler: pki_backend_ref.key_verify}
            ],
            help: "Data verification."
        });

        path
    }

    pub fn keys_encrypt_path(&self) -> Path {
        let pki_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"keys/encrypt",
            fields: {
                "key_name": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "key name"
                },
                "data": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "Data that needs to be encrypted"
                },
                "aad": {
                    field_type: FieldType::Str,
                    default: "",
                    description: "Additional Authenticated Data can be provided for aes-gcm/cbc encryption"
                }
            },
            operations: [
                {op: Operation::Write, handler: pki_backend_ref.key_encrypt}
            ],
            help: "Data encryption."
        });

        path
    }

    pub fn keys_decrypt_path(&self) -> Path {
        let pki_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"keys/decrypt",
            fields: {
                "key_name": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "key name"
                },
                "data": {
                    required: true,
                    field_type: FieldType::Str,
                    description: "Data that needs to be decrypted"
                },
                "aad": {
                    field_type: FieldType::Str,
                    default: "",
                    description: "Additional Authenticated Data can be provided for aes-gcm/cbc decryption"
                }
            },
            operations: [
                {op: Operation::Write, handler: pki_backend_ref.key_decrypt}
            ],
            help: "Data decryption."
        });

        path
    }
}

#[maybe_async::maybe_async]
impl PkiBackendInner {
    pub async fn generate_key(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let key_name_value = req.get_data("key_name")?;
        let key_name = key_name_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        let key_type_value = req.get_data_or_default("key_type")?;
        let key_type = key_type_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        let key_bits_value = req.get_data_or_default("key_bits")?;
        let key_bits = key_bits_value.as_u64().ok_or(RvError::ErrRequestFieldInvalid)?;

        let mut export_private_key = false;
        if req.path.ends_with("/exported") {
            export_private_key = true;
        }

        let key_info = self.fetch_key(req, key_name).await;
        if key_info.is_ok() {
            return Err(RvError::ErrPkiKeyNameAlreadyExist);
        }

        let mut key_bundle = KeyBundle::new(key_name, key_type.to_lowercase().as_str(), key_bits as u32);
        key_bundle.generate()?;

        self.write_key(req, &key_bundle).await?;

        let mut resp_data = json!({
            "key_id": key_bundle.id.clone(),
            "key_name": key_bundle.name.clone(),
            "key_type": key_bundle.key_type.clone(),
            "key_bits": key_bundle.bits,
        })
        .as_object()
        .unwrap()
        .clone();

        if export_private_key {
            match key_type {
                "rsa" | "ec" | "sm2" => {
                    resp_data.insert(
                        "private_key".to_string(),
                        Value::String(String::from_utf8_lossy(&key_bundle.key).to_string()),
                    );
                }
                _ => {
                    resp_data.insert("private_key".to_string(), Value::String(hex::encode(&key_bundle.key)));
                }
            }

            if !key_bundle.iv.is_empty() {
                resp_data.insert("iv".to_string(), Value::String(hex::encode(&key_bundle.iv)));
            }
        }

        Ok(Some(Response::data_response(Some(resp_data))))
    }

    pub async fn import_key(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let key_name_value = req.get_data("key_name")?;
        let key_name = key_name_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        let key_type_value = req.get_data_or_default("key_type")?;
        let key_type = key_type_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        let pem_bundle_value = req.get_data_or_default("pem_bundle")?;
        let pem_bundle = pem_bundle_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        let hex_bundle_value = req.get_data_or_default("hex_bundle")?;
        let hex_bundle = hex_bundle_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        if pem_bundle.is_empty() && hex_bundle.is_empty() {
            return Err(RvError::ErrRequestFieldNotFound);
        }

        let key_info = self.fetch_key(req, key_name).await;
        if key_info.is_ok() {
            return Err(RvError::ErrPkiKeyNameAlreadyExist);
        }

        let mut key_bundle = KeyBundle::new(key_name, key_type.to_lowercase().as_str(), 0);

        if !pem_bundle.is_empty() {
            key_bundle.key = pem_bundle.as_bytes().to_vec();
            match key_type {
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
            let iv_value = req.get_data_or_default("iv")?;
            let is_iv_required = matches!(key_type, "aes-gcm" | "aes-cbc" | "sm4-gcm" | "sm4-ccm");
            #[cfg(feature = "crypto_adaptor_tongsuo")]
            let is_valid_key_type = matches!(key_type, "aes-gcm" | "aes-cbc" | "aes-ecb" | "sm4-gcm" | "sm4-ccm");
            #[cfg(not(feature = "crypto_adaptor_tongsuo"))]
            let is_valid_key_type = matches!(key_type, "aes-gcm" | "aes-cbc" | "aes-ecb");

            // Check if the key type is valid, if not return an error.
            if !is_valid_key_type {
                return Err(RvError::ErrPkiKeyTypeInvalid);
            }

            // Proceed to check IV only if required by the key type.
            if is_iv_required {
                if let Some(iv) = iv_value.as_str() {
                    key_bundle.iv = hex::decode(iv)?;
                } else {
                    return Err(RvError::ErrRequestFieldNotFound);
                }
            }
        }

        self.write_key(req, &key_bundle).await?;

        let resp_data = json!({
            "key_id": key_bundle.id.clone(),
            "key_name": key_bundle.name.clone(),
            "key_type": key_bundle.key_type.clone(),
            "key_bits": key_bundle.bits,
        })
        .as_object()
        .cloned();

        Ok(Some(Response::data_response(resp_data)))
    }

    pub async fn key_sign(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let data_value = req.get_data("data")?;
        let data = data_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        let key_bundle =
            self.fetch_key(req, req.get_data("key_name")?.as_str().ok_or(RvError::ErrRequestFieldInvalid)?).await?;

        let decoded_data = hex::decode(data.as_bytes())?;
        let result = key_bundle.sign(&decoded_data)?;

        let resp_data = json!({
            "result": hex::encode(result),
        })
        .as_object()
        .cloned();

        Ok(Some(Response::data_response(resp_data)))
    }

    pub async fn key_verify(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let data_value = req.get_data("data")?;
        let data = data_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        let signature_value = req.get_data("signature")?;
        let signature = signature_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        let key_bundle =
            self.fetch_key(req, req.get_data("key_name")?.as_str().ok_or(RvError::ErrRequestFieldInvalid)?).await?;

        let decoded_data = hex::decode(data.as_bytes())?;
        let decoded_signature = hex::decode(signature.as_bytes())?;
        let result = key_bundle.verify(&decoded_data, &decoded_signature)?;

        let resp_data = json!({
            "result": result,
        })
        .as_object()
        .cloned();

        Ok(Some(Response::data_response(resp_data)))
    }

    pub async fn key_encrypt(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let data_value = req.get_data("data")?;
        let data = data_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        let aad_value = req.get_data_or_default("aad")?;
        let aad = aad_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        let key_bundle =
            self.fetch_key(req, req.get_data("key_name")?.as_str().ok_or(RvError::ErrRequestFieldInvalid)?).await?;

        let decoded_data = hex::decode(data.as_bytes())?;
        let result = key_bundle.encrypt(&decoded_data, Some(EncryptExtraData::Aad(aad.as_bytes())))?;

        let resp_data = json!({
            "result": hex::encode(result),
        })
        .as_object()
        .cloned();

        Ok(Some(Response::data_response(resp_data)))
    }

    pub async fn key_decrypt(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let data_value = req.get_data("data")?;
        let data = data_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        let aad_value = req.get_data_or_default("aad")?;
        let aad = aad_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        let key_bundle =
            self.fetch_key(req, req.get_data("key_name")?.as_str().ok_or(RvError::ErrRequestFieldInvalid)?).await?;

        let decoded_data = hex::decode(data.as_bytes())?;
        let result = key_bundle.decrypt(&decoded_data, Some(EncryptExtraData::Aad(aad.as_bytes())))?;

        let resp_data = json!({
            "result": hex::encode(result),
        })
        .as_object()
        .cloned();

        Ok(Some(Response::data_response(resp_data)))
    }

    pub async fn fetch_key(&self, req: &Request, key_name: &str) -> Result<KeyBundle, RvError> {
        let entry = req.storage_get(format!("{PKI_CONFIG_KEY_PREFIX}{key_name}").as_str()).await?;
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
