use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    storage::StorageEntry,
};

use super::{SSH_ROLES_PREFIX, SshBackend, SshBackendInner, types::SshRole};

impl SshBackend {
    pub fn role_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern(r"roles/(?P<name>\w[\w-]+\w)")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.write_role(backend, req).await })
                }
            })
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_role(backend, req).await })
                }
            })
            .operation(Operation::Delete, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.delete_role(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Role name"),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("user")
                    .description("Key type: user or host"),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("24h")
                    .description("Default TTL"),
            )
            .field(
                "max_ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("72h")
                    .description("Max TTL"),
            )
            .field(
                "allowed_users",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("*")
                    .description("Allowed users (comma separated)"),
            )
            .field(
                "allow_user_certificates",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value("true")
                    .description("Allow user certs"),
            )
            .field(
                "allow_host_certificates",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value("false")
                    .description("Allow host certs"),
            )
            .help("Manage SSH roles")
            .build()
    }

    pub fn roles_list_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("roles/?")
            .operation(Operation::List, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.list_roles(backend, req).await })
                }
            })
            .help("List configured SSH roles")
            .build()
    }
}

impl SshBackendInner {
    pub async fn write_role(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_val = req.get_data("name")?;
        let name = name_val
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();

        let role = SshRole {
            name: name.clone(),
            key_type: req
                .get_data("key_type")
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or("user".to_string()),
            ttl: req
                .get_data("ttl")
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or("24h".to_string()),
            max_ttl: req
                .get_data("max_ttl")
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or("72h".to_string()),
            allowed_users: req
                .get_data("allowed_users")
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or("*".to_string()),
            allow_user_certificates: req
                .get_data("allow_user_certificates")
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            allow_host_certificates: req
                .get_data("allow_host_certificates")
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        };

        let entry = StorageEntry {
            key: format!("{}{}", SSH_ROLES_PREFIX, name),
            value: serde_json::to_vec(&role)?,
        };
        req.storage_put(&entry).await?;

        Ok(None)
    }

    pub async fn read_role(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_val = req.get_data("name")?;
        let name = name_val
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();

        let entry = req
            .storage_get(&format!("{}{}", SSH_ROLES_PREFIX, name))
            .await?;
        if entry.is_none() {
            return Ok(None);
        }

        let role: SshRole = serde_json::from_slice(&entry.unwrap().value)?;
        let resp = serde_json::to_value(&role)
            .map_err(|e| RvError::ErrOther(anyhow::anyhow!("Failed to serialize role: {}", e)))?
            .as_object()
            .ok_or(RvError::ErrResponseDataInvalid)?
            .clone();

        Ok(Some(Response::data_response(Some(resp))))
    }

    pub async fn delete_role(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_val = req.get_data("name")?;
        let name = name_val
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();

        req.storage_delete(&format!("{}{}", SSH_ROLES_PREFIX, name))
            .await?;
        Ok(None)
    }

    pub async fn list_roles(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let keys = req.storage_list(SSH_ROLES_PREFIX).await?;
        Ok(Some(Response::list_response(&keys)))
    }
}
