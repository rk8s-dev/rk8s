//! The system module is mainly used to configure RustyVault itself. For instance, the 'mount/'
//! path is provided here to support mounting new modules in RustyVault via RESTful HTTP request.

use std::{
    any::Any,
    sync::{Arc, Weak},
};

use async_trait::async_trait;
use serde_json::{Map, Value, from_value, json};

use crate::{
    core::Core,
    errors::RvError,
    logical::{
        Backend, FieldBuilder, FieldType, FieldsBuilder, LogicalBackend, Operation, PathBuilder,
        Request, Response, field::FieldTrait,
    },
    modules::{
        Module,
        auth::{AUTH_TABLE_TYPE, AuthModule},
        policy::{PolicyModule, acl::ACL},
    },
    mount::{MOUNT_TABLE_TYPE, MountEntry},
    rv_error_response_status,
    storage::StorageEntry,
};

static SYSTEM_BACKEND_HELP: &str = r#"
The system backend is built-in to RustyVault and cannot be remounted or
unmounted. It contains the paths that are used to configure RustyVault itself
as well as perform core operations.
"#;

pub struct SystemModule {
    pub name: String,
    pub backend: Arc<SystemBackend>,
}

pub struct SystemBackend {
    pub core: Arc<Core>,
    pub self_ptr: Weak<SystemBackend>,
}

impl SystemBackend {
    pub fn new(core: Arc<Core>) -> Arc<Self> {
        let system_backend = SystemBackend {
            core,
            self_ptr: Weak::default(),
        };

        system_backend.wrap()
    }

    pub fn wrap(self) -> Arc<Self> {
        let mut wrap_self = Arc::new(self);
        let weak_self = Arc::downgrade(&wrap_self);
        unsafe {
            let ptr_self = Arc::into_raw(wrap_self) as *mut Self;
            (*ptr_self).self_ptr = weak_self;
            wrap_self = Arc::from_raw(ptr_self);
        }

        wrap_self
    }

    pub fn new_backend(&self) -> LogicalBackend {
        let builder = LogicalBackend::builder()
            .help(SYSTEM_BACKEND_HELP)
            .root_paths([
                "mounts/*",
                "auth/*",
                "remount",
                "policy",
                "policy/*",
                "audit",
                "audit/*",
                "seal",
                "raw/*",
                "revoke-prefix/*",
            ])
            .unauth_paths([
                "internal/ui/mounts",
                "internal/ui/mounts/*",
                "init",
                "seal-status",
                "unseal",
            ]);

        {
            let backend = self
                .self_ptr
                .upgrade()
                .expect("system backend weak reference should be valid");

            let mut paths = Vec::new();

            paths.push(
                PathBuilder::new()
                    .pattern("mounts$")
                    .operation(Operation::Read, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_mount_table(backend, req).await })
                        }
                    })
                    .build(),
            );

            let mount_fields = FieldsBuilder::new()
                .field(
                    "path",
                    FieldBuilder::new()
                        .field_type(FieldType::Str)
                        .description(r#"The path to mount to. Example: "aws/east""#),
                )
                .field(
                    "type",
                    FieldBuilder::new()
                        .field_type(FieldType::Str)
                        .description(r#"The type of the backend. Example: "kv""#),
                )
                .field(
                    "description",
                    FieldBuilder::new()
                        .field_type(FieldType::Str)
                        .default_value("")
                        .description("User-friendly description for this mount."),
                )
                .field(
                    "options",
                    FieldBuilder::new()
                        .field_type(FieldType::Map)
                        .description(
                            "The options to pass into the backend. Should be a json object with string keys and values.",
                        ),
                )
                .build();

            paths.push(
                PathBuilder::new()
                    .pattern("mounts/(?P<path>.+)")
                    .fields(mount_fields)
                    .operation(Operation::Write, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_mount(backend, req).await })
                        }
                    })
                    .operation(Operation::Delete, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_unmount(backend, req).await })
                        }
                    })
                    .build(),
            );

            let remount_fields = FieldsBuilder::new()
                .field("from", FieldBuilder::new().field_type(FieldType::Str))
                .field("to", FieldBuilder::new().field_type(FieldType::Str))
                .build();

            paths.push(
                PathBuilder::new()
                    .pattern("remount")
                    .fields(remount_fields)
                    .operation(Operation::Write, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_remount(backend, req).await })
                        }
                    })
                    .build(),
            );

            let renew_fields = FieldsBuilder::new()
                .field(
                    "lease_id",
                    FieldBuilder::new().field_type(FieldType::Str).description(
                        "The lease identifier to renew. This is included with a lease.",
                    ),
                )
                .field(
                    "increment",
                    FieldBuilder::new()
                        .field_type(FieldType::Int)
                        .description("The desired increment in seconds to the lease"),
                )
                .build();

            paths.push(
                PathBuilder::new()
                    .pattern("renew/(?P<lease_id>.+)")
                    .fields(renew_fields)
                    .operation(Operation::Write, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_renew(backend, req).await })
                        }
                    })
                    .build(),
            );

            paths.push(
                PathBuilder::new()
                    .pattern("revoke/(?P<lease_id>.+)")
                    .field(
                        "lease_id",
                        FieldBuilder::new().field_type(FieldType::Str).description(
                            "The lease identifier to renew. This is included with a lease.",
                        ),
                    )
                    .operation(Operation::Write, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_revoke(backend, req).await })
                        }
                    })
                    .build(),
            );

            paths.push(
                PathBuilder::new()
                    .pattern("revoke-prefix/(?P<prefix>.+)")
                    .field(
                        "prefix",
                        FieldBuilder::new().field_type(FieldType::Str).description(
                            r#"The path to revoke keys under. Example: "prod/aws/ops""#,
                        ),
                    )
                    .operation(Operation::Write, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(
                                async move { handler.handle_revoke_prefix(backend, req).await },
                            )
                        }
                    })
                    .build(),
            );

            paths.push(
                PathBuilder::new()
                    .pattern("auth$")
                    .operation(Operation::Read, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_auth_table(backend, req).await })
                        }
                    })
                    .build(),
            );

            let auth_fields = FieldsBuilder::new()
                .field(
                    "path",
                    FieldBuilder::new()
                        .field_type(FieldType::Str)
                        .description(
                            r#"The path to mount to. Cannot be delimited. Example: "user""#,
                        ),
                )
                .field(
                    "type",
                    FieldBuilder::new()
                        .field_type(FieldType::Str)
                        .description(
                            r#"The type of the backend. Example: "userpass""#,
                        ),
                )
                .field(
                    "description",
                    FieldBuilder::new()
                        .field_type(FieldType::Str)
                        .default_value("")
                        .description(
                            "User-friendly description for this credential backend.",
                        ),
                )
                .field(
                    "options",
                    FieldBuilder::new()
                        .field_type(FieldType::Map)
                        .description(
                            "The options to pass into the backend. Should be a json object with string keys and values.",
                        ),
                )
                .build();

            paths.push(
                PathBuilder::new()
                    .pattern("auth/(?P<path>.+)")
                    .fields(auth_fields)
                    .operation(Operation::Write, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_auth_enable(backend, req).await })
                        }
                    })
                    .operation(Operation::Delete, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_auth_disable(backend, req).await })
                        }
                    })
                    .build(),
            );

            paths.push(
                PathBuilder::new()
                    .pattern("policy/?$")
                    .operation(Operation::Read, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_policy_list(backend, req).await })
                        }
                    })
                    .operation(Operation::List, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_policy_list(backend, req).await })
                        }
                    })
                    .build(),
            );

            let policy_fields = FieldsBuilder::new()
                .field(
                    "name",
                    FieldBuilder::new()
                        .field_type(FieldType::Str)
                        .description(r#"The name of the policy. Example: "ops""#),
                )
                .field(
                    "policy",
                    FieldBuilder::new().field_type(FieldType::Str).description(
                        r#"The rules of the policy. Either given in HCL or JSON format."#,
                    ),
                )
                .build();

            paths.push(
                PathBuilder::new()
                    .pattern("policy/(?P<name>.+)")
                    .fields(policy_fields.clone())
                    .operation(Operation::Read, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_policy_read(backend, req).await })
                        }
                    })
                    .operation(Operation::Write, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_policy_write(backend, req).await })
                        }
                    })
                    .operation(Operation::Delete, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(
                                async move { handler.handle_policy_delete(backend, req).await },
                            )
                        }
                    })
                    .build(),
            );

            paths.push(
                PathBuilder::new()
                    .pattern("policies/acl/?$")
                    .operation(Operation::Read, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_policy_list(backend, req).await })
                        }
                    })
                    .operation(Operation::List, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_policy_list(backend, req).await })
                        }
                    })
                    .build(),
            );

            paths.push(
                PathBuilder::new()
                    .pattern("policies/acl/(?P<name>.+)")
                    .fields(policy_fields)
                    .operation(Operation::Read, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_policy_read(backend, req).await })
                        }
                    })
                    .operation(Operation::Write, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_policy_write(backend, req).await })
                        }
                    })
                    .operation(Operation::Delete, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(
                                async move { handler.handle_policy_delete(backend, req).await },
                            )
                        }
                    })
                    .build(),
            );

            paths.push(
                PathBuilder::new()
                    .pattern("audit$")
                    .operation(Operation::Read, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_audit_table(backend, req).await })
                        }
                    })
                    .build(),
            );

            let audit_fields = FieldsBuilder::new()
                .field(
                    "path",
                    FieldBuilder::new().field_type(FieldType::Str).description(
                        r#"The name of the backend. Cannot be delimited. Example: "mysql""#,
                    ),
                )
                .field(
                    "type",
                    FieldBuilder::new()
                        .field_type(FieldType::Str)
                        .description(r#"The type of the backend. Example: "mysql""#),
                )
                .field(
                    "description",
                    FieldBuilder::new()
                        .field_type(FieldType::Str)
                        .description("User-friendly description for this audit backend."),
                )
                .field(
                    "options",
                    FieldBuilder::new()
                        .field_type(FieldType::Map)
                        .description("Configuration options for the audit backend."),
                )
                .build();

            paths.push(
                PathBuilder::new()
                    .pattern("audit/(?P<path>.+)")
                    .fields(audit_fields)
                    .operation(Operation::Write, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_audit_enable(backend, req).await })
                        }
                    })
                    .operation(Operation::Delete, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(
                                async move { handler.handle_audit_disable(backend, req).await },
                            )
                        }
                    })
                    .build(),
            );

            let raw_fields = FieldsBuilder::new()
                .field("path", FieldBuilder::new().field_type(FieldType::Str))
                .field("value", FieldBuilder::new().field_type(FieldType::Str))
                .build();

            paths.push(
                PathBuilder::new()
                    .pattern("raw/(?P<path>.+)")
                    .fields(raw_fields)
                    .operation(Operation::Read, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_raw_read(backend, req).await })
                        }
                    })
                    .operation(Operation::Write, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_raw_write(backend, req).await })
                        }
                    })
                    .operation(Operation::Delete, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move { handler.handle_raw_delete(backend, req).await })
                        }
                    })
                    .build(),
            );

            paths.push(
                PathBuilder::new()
                    .pattern("internal/ui/mounts")
                    .operation(Operation::Read, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move {
                                handler.handle_internal_ui_mounts_read(backend, req).await
                            })
                        }
                    })
                    .build(),
            );

            paths.push(
                PathBuilder::new()
                    .pattern("internal/ui/mounts/(?P<path>.+)")
                    .field(
                        "path",
                        FieldBuilder::new()
                            .field_type(FieldType::Str)
                            .description("The path of the mount."),
                    )
                    .operation(Operation::Read, {
                        let handler = backend.clone();

                        move |backend, req| {
                            let handler = handler.clone();

                            Box::pin(async move {
                                handler.handle_internal_ui_mount_read(backend, req).await
                            })
                        }
                    })
                    .build(),
            );

            builder.paths(paths).build()
        }
    }

    pub async fn handle_mount_table(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let mut data: Map<String, Value> = Map::new();

        let mounts = self.core.mounts_router.entries.read()?;

        for mount_entry in mounts.values() {
            let entry = mount_entry.read()?;
            let info: Value = json!({
                "type": entry.logical_type.clone(),
                "description": entry.description.clone(),
            });
            data.insert(entry.path.clone(), info);
        }

        Ok(Some(Response::data_response(Some(data))))
    }

    pub async fn handle_mount(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let path = req.get_data("path")?;
        let logical_type = req.get_data("type")?;
        let description = req.get_data_or_default("description")?;
        let options = req.get_data_or_default("options")?;

        let path = path.as_str().unwrap();
        let logical_type = logical_type.as_str().unwrap();
        let description = description.as_str().unwrap();

        if logical_type.is_empty() {
            return Err(RvError::ErrRequestInvalid);
        }

        let mut me = MountEntry::new(MOUNT_TABLE_TYPE, path, logical_type, description);
        me.options = options.as_map();

        self.core.mount(&me).await?;
        Ok(None)
    }

    pub async fn handle_unmount(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let suffix = req.path.trim_start_matches("mounts/");
        if suffix.is_empty() {
            return Err(RvError::ErrRequestInvalid);
        }

        self.core.unmount(suffix).await?;
        Ok(None)
    }

    pub async fn handle_remount(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let from = req.get_data("from")?;
        let to = req.get_data("to")?;

        let from = from.as_str().unwrap();
        let to = to.as_str().unwrap();
        if from.is_empty() || to.is_empty() {
            return Err(RvError::ErrRequestInvalid);
        }

        let from_path = sanitize_path(from);
        let to_path = sanitize_path(to);

        if let Some(me) = self.core.router.matching_mount_entry(&from_path)? {
            let mount_entry_table_type;
            {
                let mount_entry = me.read()?;

                let dst_path_match = self.core.router.matching_mount(to)?;
                if !dst_path_match.is_empty() {
                    return Err(rv_error_response_status!(
                        409,
                        &format!("path already in use at {dst_path_match}")
                    ));
                }

                mount_entry_table_type = mount_entry.table.clone();

                std::mem::drop(mount_entry);
            }

            match mount_entry_table_type.as_str() {
                AUTH_TABLE_TYPE => {
                    let auth_module = self.get_module::<AuthModule>("auth")?;
                    auth_module.remount_auth(&from_path, &to_path).await?;
                }
                MOUNT_TABLE_TYPE => {
                    self.core.remount(&from_path, &to_path).await?;
                }
                _ => {
                    return Err(rv_error_response_status!(409, "Unknown mount table type."));
                }
            }
        } else {
            return Err(rv_error_response_status!(
                409,
                &format!("no matching mount at {from_path}")
            ));
        }

        Ok(None)
    }

    pub async fn handle_renew(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let _lease_id = req.get_data("lease_id")?;
        let _increment: i32 = from_value(req.get_data("increment")?)?;
        //TODO
        Ok(None)
    }

    pub async fn handle_revoke(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let _lease_id = req.get_data("lease_id")?;
        //TODO
        Ok(None)
    }

    pub async fn handle_revoke_prefix(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let _prefix = req.get_data("prefix")?;
        Ok(None)
    }

    pub async fn handle_auth_table(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let mut data: Map<String, Value> = Map::new();

        let auth_module = self.get_module::<AuthModule>("auth")?;

        let mounts = auth_module.mounts_router.entries.read()?;

        for mount_entry in mounts.values() {
            let entry = mount_entry.read()?;
            let info: Value = json!({
                "type": entry.logical_type.clone(),
                "description": entry.description.clone(),
            });
            data.insert(entry.path.clone(), info);
        }

        Ok(Some(Response::data_response(Some(data))))
    }

    pub async fn handle_auth_enable(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let path = req.get_data("path")?;
        let logical_type = req.get_data("type")?;
        let description = req.get_data_or_default("description")?;
        let options = req.get_data_or_default("options")?;

        let path = sanitize_path(path.as_str().unwrap());
        let logical_type = logical_type.as_str().unwrap();
        let description = description.as_str().unwrap();

        if logical_type.is_empty() {
            return Err(RvError::ErrRequestInvalid);
        }

        let mut me = MountEntry::new(AUTH_TABLE_TYPE, &path, logical_type, description);

        me.options = options.as_map();

        let auth_module = self.get_module::<AuthModule>("auth")?;

        auth_module.enable_auth(&me).await?;

        Ok(None)
    }

    pub async fn handle_auth_disable(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let path = sanitize_path(req.path.trim_start_matches("auth/"));
        if path.is_empty() {
            return Err(RvError::ErrRequestInvalid);
        }

        let auth_module = self.get_module::<AuthModule>("auth")?;

        auth_module.disable_auth(&path).await?;

        Ok(None)
    }

    pub async fn handle_policy_list(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let policy_module = self.get_module::<PolicyModule>("policy")?;

        policy_module.handle_policy_list(backend, req).await
    }

    pub async fn handle_policy_read(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let policy_module = self.get_module::<PolicyModule>("policy")?;

        policy_module.handle_policy_read(backend, req).await
    }

    pub async fn handle_policy_write(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let policy_module = self.get_module::<PolicyModule>("policy")?;

        policy_module.handle_policy_write(backend, req).await
    }

    pub async fn handle_policy_delete(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let policy_module = self.get_module::<PolicyModule>("policy")?;

        policy_module.handle_policy_delete(backend, req).await
    }

    pub async fn handle_audit_table(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    pub async fn handle_audit_enable(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    pub async fn handle_audit_disable(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    pub async fn handle_raw_read(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let path = req.get_data("path")?;

        let path = path.as_str().unwrap();

        let entry = self.core.barrier.get(path).await?;
        if entry.is_none() {
            return Ok(None);
        }

        let data = json!({
            "value": String::from_utf8_lossy(&entry.unwrap().value),
        })
        .as_object()
        .cloned();

        Ok(Some(Response::data_response(data)))
    }

    pub async fn handle_raw_write(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let path = req.get_data("path")?;
        let value = req.get_data("value")?;

        let path = path.as_str().unwrap();
        let value = value.as_str().unwrap();

        let entry = StorageEntry {
            key: path.to_string(),
            value: value.as_bytes().to_vec(),
        };

        self.core.barrier.put(&entry).await?;

        Ok(None)
    }

    pub async fn handle_raw_delete(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let path = req.get_data("path")?;

        let path = path.as_str().unwrap();

        self.core.barrier.delete(path).await?;

        Ok(None)
    }

    pub async fn handle_internal_ui_mounts_read(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let policy_module = self.get_module::<PolicyModule>("policy")?;
        let auth_module = self.get_module::<AuthModule>("auth")?;

        let Some(token_store) = auth_module.token_store.load_full() else {
            return Err(RvError::ErrPermissionDenied);
        };

        let mut secret_mounts = Map::new();
        let mut auth_mounts = Map::new();

        let mut is_authed = false;

        let acl: Option<ACL> = if let Some(auth) = token_store
            .check_token(&req.path, &req.client_token)
            .await?
        {
            if auth.policies.is_empty() {
                None
            } else {
                is_authed = true;
                Some(
                    policy_module
                        .policy_store
                        .load()
                        .new_acl(&auth.policies, None)
                        .await?,
                )
            }
        } else {
            None
        };

        let has_access = |me: &MountEntry| -> bool {
            if !is_authed {
                return false;
            }

            let Some(acl) = acl.as_ref() else {
                return false;
            };

            if me.table == AUTH_TABLE_TYPE {
                acl.has_mount_access(&format!("{}/{}", AUTH_TABLE_TYPE, me.path))
            } else {
                acl.has_mount_access(me.path.as_str())
            }
        };

        let entries = self.core.mounts_router.entries.read()?;
        for (path, entry) in entries.iter() {
            let me = entry.read()?;
            if has_access(&me) {
                if is_authed {
                    secret_mounts.insert(path.clone(), Value::Object(self.mount_info(&me)));
                } else {
                    secret_mounts.insert(
                        path.clone(),
                        json!({
                            "type": me.logical_type.clone(),
                            "description": me.description.clone(),
                            "options": me.options.clone(),
                        }),
                    );
                }
            }
        }

        let entries = auth_module.mounts_router.entries.read()?;
        for (path, entry) in entries.iter() {
            let me = entry.read()?;
            if has_access(&me) {
                if is_authed {
                    auth_mounts.insert(path.clone(), Value::Object(self.mount_info(&me)));
                } else {
                    auth_mounts.insert(
                        path.clone(),
                        json!({
                            "type": me.logical_type.clone(),
                            "description": me.description.clone(),
                            "options": me.options.clone(),
                        }),
                    );
                }
            }
        }

        let data = json!({
            "secret": secret_mounts,
            "auth": auth_mounts,
        })
        .as_object()
        .cloned();

        Ok(Some(Response::data_response(data)))
    }

    pub async fn handle_internal_ui_mount_read(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let policy_module = self.get_module::<PolicyModule>("policy")?;
        let auth_module = self.get_module::<AuthModule>("auth")?;

        let path = sanitize_path(
            req.get_data("path")?
                .as_str()
                .ok_or(RvError::ErrRequestInvalid)?,
        );

        if auth_module.token_store.load().is_none() {
            return Err(RvError::ErrPermissionDenied);
        }

        let acl = if let Some(auth) = auth_module
            .token_store
            .load()
            .as_ref()
            .unwrap()
            .check_token(&req.path, &req.client_token)
            .await?
        {
            if auth.policies.is_empty() {
                return Err(RvError::ErrPermissionDenied);
            } else {
                policy_module
                    .policy_store
                    .load()
                    .new_acl(&auth.policies, None)
                    .await?
            }
        } else {
            return Err(RvError::ErrPermissionDenied);
        };

        let mount_entry = self
            .core
            .mounts_router
            .router
            .matching_mount_entry(&path)?
            .ok_or(RvError::ErrPermissionDenied)?;
        let me = mount_entry.read()?;

        let full_path = if me.table == AUTH_TABLE_TYPE {
            &format!("{}/{}", AUTH_TABLE_TYPE, me.path)
        } else {
            &me.path
        };

        if !acl.has_mount_access(full_path) {
            return Err(RvError::ErrPermissionDenied);
        }

        let mut data = self.mount_info(&me);
        data.insert("path".to_string(), Value::String(me.path.clone()));

        Ok(Some(Response::data_response(Some(data))))
    }

    fn get_module<T: Any + Send + Sync>(&self, name: &str) -> Result<Arc<T>, RvError> {
        if let Some(module) = self.core.module_manager.get_module::<T>(name) {
            return Ok(module);
        }

        Err(RvError::ErrModuleNotFound)
    }

    fn mount_info(&self, entry: &MountEntry) -> Map<String, Value> {
        let info = json!({
            "type": entry.logical_type.clone(),
            "description": entry.description.clone(),
            "uuid": entry.uuid.clone(),
            "options": entry.options.clone(),
        })
        .as_object()
        .unwrap()
        .clone();

        info.clone()
    }
}

impl SystemModule {
    pub fn new(core: Arc<Core>) -> Self {
        Self {
            name: "system".to_string(),
            backend: SystemBackend::new(core),
        }
    }
}

#[async_trait]
impl Module for SystemModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn setup(&self, core: &Core) -> Result<(), RvError> {
        let sys = self.backend.clone();
        let sys_backend_new_func = move |_c: Arc<Core>| -> Result<Arc<dyn Backend>, RvError> {
            let mut sys_backend = sys.new_backend();
            sys_backend.init()?;
            Ok(Arc::new(sys_backend))
        };
        core.add_logical_backend("system", Arc::new(sys_backend_new_func))
    }

    fn cleanup(&self, core: &Core) -> Result<(), RvError> {
        core.delete_logical_backend("system")
    }
}

fn sanitize_path(path: &str) -> String {
    let mut new_path = path.to_string();
    if !new_path.ends_with('/') {
        new_path.push('/');
    }
    if new_path.starts_with('/') {
        new_path = new_path[1..].to_string();
    }
    new_path
}
