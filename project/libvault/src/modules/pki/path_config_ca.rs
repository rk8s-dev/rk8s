use openssl::{
    pkey::{Id, PKey},
    x509::X509,
};
use pem;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::{PkiBackend, PkiBackendInner, ssh_util, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::{RequestExt, ResponseExt},
    storage::StorageEntry,
    utils::{cert, cert::CertBundle},
};

const SSH_CA_BUNDLE_KEY: &str = "config/ca/ssh";

/// Stored SSH CA key pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshCaBundle {
    pub key_type: String,
    pub key_bits: u32,
    pub private_key_pem: String,
    pub public_key_openssh: String,
}

impl PkiBackend {
    pub fn config_ca_path(&self) -> Path {
        let backend_write = self.inner.clone();
        let backend_read = self.inner.clone();
        let backend_delete = self.inner.clone();

        Path::builder()
            .pattern(r"config/ca/(?P<cert_type>tls|ssh)")
            .field(
                "cert_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Certificate type: tls or ssh"),
            )
            // TLS fields
            .field(
                "pem_bundle",
                Field::builder().field_type(FieldType::Str).description(
                    "PEM-format, concatenated unencrypted secret key and certificate (TLS)",
                ),
            )
            // SSH fields
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("rsa")
                    .description("SSH CA key type: rsa, ec, or ed25519"),
            )
            .field(
                "key_bits",
                Field::builder()
                    .field_type(FieldType::Int)
                    .default_value(0)
                    .description("Key bits (0 for default)"),
            )
            .field(
                "private_key",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Optional PEM-encoded private key to import (SSH)"),
            )
            .field(
                "public_key",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Optional OpenSSH public key to import (SSH)"),
            )
            .operation(Operation::Write, {
                let handler = backend_write.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.write_config_ca(backend, req).await })
                }
            })
            .operation(Operation::Read, {
                let handler = backend_read.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_config_ca(backend, req).await })
                }
            })
            .operation(Operation::Delete, {
                let handler = backend_delete.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.delete_config_ca(backend, req).await })
                }
            })
            .help("Configure the CA for TLS or SSH.")
            .build()
    }
}

impl PkiBackendInner {
    // ── Unified dispatch ──

    pub async fn write_config_ca(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let cert_type = req.get_data("cert_type")?;
        let cert_type = cert_type.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        match cert_type {
            "tls" => self.write_path_ca(backend, req).await,
            "ssh" => self.write_ssh_config_ca(backend, req).await,
            _ => Err(RvError::ErrRequestFieldInvalid),
        }
    }

    pub async fn read_config_ca(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let cert_type = req.get_data("cert_type")?;
        let cert_type = cert_type.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        match cert_type {
            "tls" => {
                // TLS CA read: return the CA bundle info
                let ca_bundle = self.fetch_ca_bundle(req).await?;
                let response = types::FetchCaResponse {
                    certificate: String::from_utf8_lossy(&ca_bundle.certificate.to_pem()?)
                        .to_string(),
                    ca_chain: None,
                    issuing_ca: None,
                    serial_number: Some(ca_bundle.serial_number.clone()),
                };
                Ok(Some(Response::data_response(response.to_map()?)))
            }
            "ssh" => self.read_ssh_config_ca(backend, req).await,
            _ => Err(RvError::ErrRequestFieldInvalid),
        }
    }

    pub async fn delete_config_ca(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let cert_type = req.get_data("cert_type")?;
        let cert_type = cert_type.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        match cert_type {
            "tls" => {
                self.delete_ca_bundle(req).await?;
                Ok(None)
            }
            "ssh" => self.delete_ssh_config_ca(backend, req).await,
            _ => Err(RvError::ErrRequestFieldInvalid),
        }
    }

    // ── TLS CA ──

    pub async fn write_path_ca(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::ConfigCaRequest = req.parse_json()?;
        let pem_bundle = payload.pem_bundle.as_str();

        let items = pem::parse_many(pem_bundle)?;
        let mut key_found = false;
        let mut i = 0;

        let mut cert_bundle = CertBundle::default();

        for item in items {
            if item.tag() == "CERTIFICATE" {
                let cert = X509::from_der(item.contents())?;
                if !cert::is_ca_cert(&cert) {
                    return Err(RvError::ErrPkiPemBundleInvalid);
                }

                if i == 0 {
                    cert_bundle.certificate = cert;
                } else {
                    cert_bundle.ca_chain.push(cert);
                }
                i += 1;
            }
            if item.tag() == "PRIVATE KEY" {
                if key_found {
                    return Err(RvError::ErrPkiPemBundleInvalid);
                }

                let key = PKey::private_key_from_der(item.contents())?;
                match key.id() {
                    Id::RSA => {
                        cert_bundle.private_key_type = "rsa".to_string();
                    }
                    Id::EC => {
                        cert_bundle.private_key_type = "ec".to_string();
                    }
                    Id::SM2 => {
                        cert_bundle.private_key_type = "sm2".to_string();
                    }
                    Id::ED25519 => {
                        cert_bundle.private_key_type = "ed25519".to_string();
                    }
                    _ => {
                        cert_bundle.private_key_type = "other".to_string();
                    }
                }
                cert_bundle.private_key = key;
                key_found = true;
            }
        }

        cert_bundle.verify()?;

        self.store_ca_bundle(req, &cert_bundle).await?;

        let entry = StorageEntry {
            key: "crl".to_string(),
            value: Vec::new(),
        };

        req.storage_put(&entry).await?;

        Ok(None)
    }

    pub async fn fetch_ca_bundle(&self, req: &Request) -> Result<CertBundle, RvError> {
        let entry = req.storage_get("config/ca/tls").await?;
        if entry.is_none() {
            return Err(RvError::ErrPkiCaNotConfig);
        }

        let ca_bundle: CertBundle = serde_json::from_slice(entry.unwrap().value.as_slice())?;
        Ok(ca_bundle)
    }

    pub async fn store_ca_bundle(
        &self,
        req: &mut Request,
        ca_bundle: &CertBundle,
    ) -> Result<(), RvError> {
        let mut entry = StorageEntry::new("config/ca/tls", ca_bundle)?;

        req.storage_put(&entry).await?;

        entry.key = "ca/tls".to_string();
        entry.value = ca_bundle.certificate.to_pem().unwrap();
        req.storage_put(&entry).await?;

        let serial_number_hex = ca_bundle.serial_number.replace(':', "-").to_lowercase();
        self.store_cert(req, &serial_number_hex, &ca_bundle.certificate)
            .await?;

        Ok(())
    }

    pub async fn delete_ca_bundle(&self, req: &Request) -> Result<(), RvError> {
        let ca_bundle = self.fetch_ca_bundle(req).await?;
        let serial_number_hex = ca_bundle.serial_number.replace(':', "-").to_lowercase();

        self.delete_cert(req, &serial_number_hex).await?;

        req.storage_delete("config/ca/tls").await?;

        Ok(())
    }

    // ── SSH CA ──

    pub async fn write_ssh_config_ca(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::SshConfigCaRequest = req.parse_json()?;
        let key_type = payload.key_type.unwrap_or_else(|| "rsa".to_string());
        let key_bits = payload.key_bits.unwrap_or(0);

        let (pkey, nid) = if let Some(ref priv_pem) = payload.private_key {
            let pkey = PKey::private_key_from_pem(priv_pem.as_bytes())?;
            let nid = if pkey.id() == openssl::pkey::Id::EC {
                pkey.ec_key()?.group().curve_name()
            } else {
                None
            };
            if let Some(ref supplied_pub) = payload.public_key {
                let derived_pub = ssh_util::format_openssh_pubkey(&pkey, nid)?;
                let supplied_key_part = supplied_pub
                    .trim()
                    .splitn(3, ' ')
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ");
                let derived_key_part = derived_pub
                    .trim()
                    .splitn(3, ' ')
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ");
                if supplied_key_part != derived_key_part {
                    return Err(RvError::ErrPkiSshPublicKeyInvalid);
                }
            }
            (pkey, nid)
        } else {
            ssh_util::generate_ssh_keypair(&key_type, key_bits)?
        };

        let public_key_openssh = ssh_util::format_openssh_pubkey(&pkey, nid)?;
        let private_key_pem =
            String::from_utf8_lossy(&pkey.private_key_to_pem_pkcs8()?).to_string();

        let bundle = SshCaBundle {
            key_type: key_type.clone(),
            key_bits,
            private_key_pem,
            public_key_openssh: public_key_openssh.clone(),
        };

        let entry = StorageEntry::new(SSH_CA_BUNDLE_KEY, &bundle)?;
        req.storage_put(&entry).await?;

        info!(
            key_type = %key_type,
            key_bits = key_bits,
            imported = payload.private_key.is_some(),
            "SSH CA key configured"
        );

        let response = types::SshConfigCaResponse {
            public_key: public_key_openssh,
        };
        Ok(Some(Response::data_response(response.to_map()?)))
    }

    pub async fn read_ssh_config_ca(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let bundle = self.fetch_ssh_ca_bundle(req).await?;
        let response = types::SshConfigCaResponse {
            public_key: bundle.public_key_openssh,
        };
        Ok(Some(Response::data_response(response.to_map()?)))
    }

    pub async fn delete_ssh_config_ca(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        req.storage_delete(SSH_CA_BUNDLE_KEY).await?;
        Ok(None)
    }

    pub async fn fetch_ssh_ca_bundle(&self, req: &Request) -> Result<SshCaBundle, RvError> {
        let entry = req.storage_get(SSH_CA_BUNDLE_KEY).await?;
        if entry.is_none() {
            return Err(RvError::ErrPkiSshCaNotConfig);
        }
        let bundle: SshCaBundle = serde_json::from_slice(entry.unwrap().value.as_slice())?;
        Ok(bundle)
    }
}
