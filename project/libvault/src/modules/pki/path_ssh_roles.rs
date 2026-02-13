use std::time::Duration;

use humantime::parse_duration;

use super::{PkiBackend, PkiBackendInner, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::{RequestExt, ResponseExt},
    storage::StorageEntry,
};

impl PkiBackend {
    pub fn ssh_roles_path(&self) -> Path {
        let backend_read = self.inner.clone();
        let backend_write = self.inner.clone();
        let backend_delete = self.inner.clone();

        Path::builder()
            .pattern(r"ssh/roles/(?P<name>\w[\w-]+\w)")
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Name of the SSH role."),
            )
            .field(
                "cert_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("user")
                    .description("Certificate type: user or host"),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("rsa")
                    .description("Key type: rsa, ec, or ed25519"),
            )
            // PLACEHOLDER_SSH_ROLES_CONTINUE
            .field(
                "key_bits",
                Field::builder()
                    .field_type(FieldType::Int)
                    .default_value(2048)
                    .description("Key bits"),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("1h")
                    .description("Certificate TTL"),
            )
            .field(
                "allowed_users",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Comma-separated list of allowed users"),
            )
            .operation(Operation::Read, {
                let handler = backend_read.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_ssh_role(backend, req).await })
                }
            })
            .operation(Operation::Write, {
                let handler = backend_write.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.create_ssh_role(backend, req).await })
                }
            })
            .operation(Operation::Delete, {
                let handler = backend_delete.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.delete_ssh_role(backend, req).await })
                }
            })
            .help("Manage SSH roles for certificate issuance.")
            .build()
    }
}

impl PkiBackendInner {
    pub async fn get_ssh_role(
        &self,
        req: &mut Request,
        name: &str,
    ) -> Result<Option<types::SshRoleEntry>, RvError> {
        let key = format!("ssh/role/{name}");
        let entry = req.storage_get(&key).await?;
        if entry.is_none() {
            return Ok(None);
        }
        let role: types::SshRoleEntry = serde_json::from_slice(entry.unwrap().value.as_slice())?;
        Ok(Some(role))
    }

    pub async fn read_ssh_role(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_value = req.get_data("name")?;
        let name = name_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        let role = self.get_ssh_role(req, name).await?;
        let data = serde_json::to_value(role)?;
        Ok(Some(Response::data_response(Some(
            data.as_object().unwrap().clone(),
        ))))
    }

    pub async fn create_ssh_role(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_value = req.get_data("name")?;
        let name = name_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        let cert_type = req
            .get_data_or_default("cert_type")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        if cert_type != "user" && cert_type != "host" {
            return Err(RvError::ErrPkiSshCertTypeInvalid);
        }

        let key_type = req
            .get_data_or_default("key_type")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();

        let key_bits = req
            .get_data_or_default("key_bits")?
            .as_u64()
            .ok_or(RvError::ErrRequestFieldInvalid)? as u32;

        let ttl_str = req
            .get_data_or_default("ttl")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let ttl = parse_duration(&ttl_str)?;

        let mut allowed_users = Vec::new();
        if let Ok(users_val) = req.get_data("allowed_users") {
            if let Some(users_str) = users_val.as_str() {
                if !users_str.is_empty() {
                    allowed_users = users_str.split(',').map(|s| s.trim().to_string()).collect();
                }
            }
        }

        let role = types::SshRoleEntry {
            cert_type,
            key_type,
            key_bits,
            ttl,
            allowed_users,
            ..Default::default()
        };

        let entry = StorageEntry::new(format!("ssh/role/{name}").as_str(), &role)?;
        req.storage_put(&entry).await?;

        Ok(None)
    }

    pub async fn delete_ssh_role(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_value = req.get_data("name")?;
        let name = name_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        req.storage_delete(format!("ssh/role/{name}").as_str())
            .await?;
        Ok(None)
    }
}
