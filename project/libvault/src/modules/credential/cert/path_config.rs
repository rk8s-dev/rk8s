use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

use super::{CertBackend, CertBackendInner};
use crate::{
    context::Context,
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, PathOperation, Request, Response},
    new_fields, new_fields_internal, new_path, new_path_internal,
    storage::StorageEntry,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub disable_binding: bool,
    pub enable_identity_alias_metadata: bool,
    pub ocsp_cache_size: i64,
}

impl CertBackend {
    pub fn config_path(&self) -> Path {
        let cert_backend_ref1 = self.inner.clone();
        let cert_backend_ref2 = self.inner.clone();

        let path = new_path!({
            pattern: r"config",
            fields: {
                "disable_binding": {
                    field_type: FieldType::Bool,
                    default: false,
                    description: r#"If set, during renewal, skips the matching of presented client identity with the client identity used during login. Defaults to false."#
                },
                "enable_identity_alias_metadata": {
                    field_type: FieldType::Bool,
                    default: false,
                    description: r#"If set, metadata of the certificate including the metadata corresponding to allowed_metadata_extensions will be stored in the alias. Defaults to false."#
                },
                "ocsp_cache_size": {
                    field_type: FieldType::Int,
                    default: 100,
                    description: "The size of the in memory OCSP response cache, shared by all configured certs"
                }
            },
            operations: [
                {op: Operation::Read, handler: cert_backend_ref1.read_config},
                {op: Operation::Write, handler: cert_backend_ref2.write_config}
            ],
            help: r#"
This endpoint allows you to create, read, update, and delete trusted certificates
that are allowed to authenticate.

Deleting a certificate will not revoke auth for prior authenticated connections.
To do this, do a revoke on "login". If you don'log need to revoke login immediately,
then the next renew will cause the lease to expire.
                "#
        });

        path
    }
}

#[maybe_async::maybe_async]
impl CertBackendInner {
    pub async fn get_config(&self, req: &Request) -> Result<Option<Config>, RvError> {
        let storage_entry = req.storage_get("config").await?;
        if storage_entry.is_none() {
            return Ok(Some(Config::default()));
        }

        let entry = storage_entry.unwrap();
        let config: Config = serde_json::from_slice(entry.value.as_slice())?;
        Ok(Some(config))
    }

    pub async fn set_config(&self, req: &mut Request, config: &Config) -> Result<(), RvError> {
        let entry = StorageEntry::new("config", config)?;

        req.storage_put(&entry).await
    }

    pub async fn read_config(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let config = self.get_config(req).await?;
        if config.is_none() {
            return Ok(None);
        }

        let cfg_data = serde_json::to_value(config.unwrap())?;

        Ok(Some(Response::data_response(Some(cfg_data.as_object().unwrap().clone()))))
    }

    pub async fn write_config(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let config = self.get_config(req).await?;
        if config.is_none() {
            return Ok(None);
        }

        let mut cfg = config.unwrap();

        if let Ok(disable_binding_raw) = req.get_data("disable_binding") {
            cfg.disable_binding = disable_binding_raw.as_bool().unwrap();
        }

        if let Ok(enable_identity_alias_metadata_raw) = req.get_data("enable_identity_alias_metadata") {
            cfg.enable_identity_alias_metadata = enable_identity_alias_metadata_raw.as_bool().unwrap();
        }

        if let Ok(ocsp_cache_size_raw) = req.get_data("ocsp_cache_size") {
            let ocsp_cache_size = ocsp_cache_size_raw.as_i64().unwrap();
            if ocsp_cache_size < 2 {
                log::error!("invalid cache size, must be >= 2 and <= max_cache_size");
                return Err(RvError::ErrRequestInvalid);
            }
            cfg.ocsp_cache_size = ocsp_cache_size;
        }

        self.set_config(req, &cfg).await?;

        Ok(None)
    }
}
