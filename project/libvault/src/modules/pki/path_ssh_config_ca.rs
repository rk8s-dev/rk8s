use openssl::pkey::PKey;
use serde::{Deserialize, Serialize};
use tracing::info;

use super::{PkiBackend, PkiBackendInner, ssh_util, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::{RequestExt, ResponseExt},
    storage::StorageEntry,
};

const SSH_CA_BUNDLE_KEY: &str = "ssh/config/ca_bundle";

/// Stored SSH CA key pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshCaBundle {
    pub key_type: String,
    pub key_bits: u32,
    pub private_key_pem: String,
    pub public_key_openssh: String,
}

impl PkiBackend {
    pub fn ssh_config_ca_path(&self) -> Path {
        let backend_write = self.inner.clone();
        let backend_read = self.inner.clone();
        let backend_delete = self.inner.clone();

        Path::builder()
            .pattern("ssh/config/ca")
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
                    .description("Optional PEM-encoded private key to import"),
            )
            .field(
                "public_key",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Optional OpenSSH public key to import"),
            )
            .operation(Operation::Write, {
                let handler = backend_write.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.write_ssh_config_ca(backend, req).await })
                }
            })
            .operation(Operation::Read, {
                let handler = backend_read.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_ssh_config_ca(backend, req).await })
                }
            })
            .operation(Operation::Delete, {
                let handler = backend_delete.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.delete_ssh_config_ca(backend, req).await })
                }
            })
            .help("Configure the SSH CA key pair.")
            .build()
    }
}

impl PkiBackendInner {
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
