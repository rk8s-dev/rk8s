use std::{collections::HashMap, sync::Arc, time::Duration};

use derive_more::{Deref, DerefMut};
use serde::{Deserialize, Serialize};

use super::{UserPassBackend, UserPassBackendInner};
use crate::{
    context::Context,
    errors::RvError,
    logical::{field::FieldTrait, Backend, Field, FieldType, Operation, Path, PathOperation, Request, Response},
    new_fields, new_fields_internal, new_path, new_path_internal,
    storage::StorageEntry,
    utils::{
        deserialize_duration, serialize_duration,
        sock_addr::SockAddrMarshaler,
        token_util::{token_fields, TokenParams},
    },
};

//const DEFAULT_MAX_TTL: Duration = Duration::from_secs(365*24*60*60 as u64);

#[derive(Debug, Clone, Default, Serialize, Deserialize, Deref, DerefMut)]
pub struct UserEntry {
    pub password_hash: String,
    pub policies: Vec<String>,
    #[serde(serialize_with = "serialize_duration", deserialize_with = "deserialize_duration")]
    pub ttl: Duration,
    #[serde(serialize_with = "serialize_duration", deserialize_with = "deserialize_duration")]
    pub max_ttl: Duration,
    #[serde(flatten, default)]
    #[deref]
    #[deref_mut]
    pub token_params: TokenParams,
    #[serde(default)]
    pub bound_cidrs: Vec<SockAddrMarshaler>,
}

#[maybe_async::maybe_async]
impl UserPassBackend {
    pub fn users_path(&self) -> Path {
        let userpass_backend_ref1 = self.inner.clone();
        let userpass_backend_ref2 = self.inner.clone();
        let userpass_backend_ref3 = self.inner.clone();

        let mut path = new_path!({
            pattern: r"users/(?P<username>\w[\w-]+\w)",
            fields: {
                "username": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Username of the user."
                },
                "password": {
                    field_type: FieldType::SecretStr,
                    required: false,
                    description: "Password for this user."
                },
                "policies": {
                    field_type: FieldType::Str,
                    required: false,
                    description: "Policies for this user."
                },
                "ttl": {
                    field_type: FieldType::Int,
                    default: 0,
                    description: "TTL for this user."
                },
                "max_ttl": {
                    field_type: FieldType::Int,
                    default: 0,
                    description: "TTL for this user."
                }
            },
            operations: [
                {op: Operation::Read, handler: userpass_backend_ref1.read_user},
                {op: Operation::Write, handler: userpass_backend_ref2.write_user},
                {op: Operation::Delete, handler: userpass_backend_ref3.delete_user}
            ],
            help: r#"
This endpoint allows you to create, read, update, and delete users
that are allowed to authenticate.
Deleting a user will not revoke auth for prior authenticated users
with that name. To do this, do a revoke on "login/<username>" for
the username you want revoked. If you don't need to revoke login immediately,
then the next renew will cause the lease to expire.
                "#
        });

        path.fields.extend(token_fields());

        path
    }

    pub fn user_list_path(&self) -> Path {
        let userpass_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"users/?",
            operations: [
                {op: Operation::List, handler: userpass_backend_ref.list_user}
            ],
            help: r#"This endpoint allows you to list users"#
        });

        path
    }

    pub fn user_password_path(&self) -> Path {
        let userpass_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"users/(?P<username>\w[\w-]+\w)/password$",
            fields: {
                "username": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Username of the user."
                },
                "password": {
                    field_type: FieldType::SecretStr,
                    required: true,
                    description: "Password for this user."
                }
            },
            operations: [
                {op: Operation::Write, handler: userpass_backend_ref.write_user_password}
            ],
            help: r#"This endpoint allows resetting the user's password."#
        });

        path
    }
}

#[allow(clippy::assigning_clones)]
#[maybe_async::maybe_async]
impl UserPassBackendInner {
    pub async fn get_user(&self, req: &mut Request, name: &str) -> Result<Option<UserEntry>, RvError> {
        let key = format!("user/{}", name.to_lowercase());
        let storage_entry = req.storage_get(&key).await?;
        if storage_entry.is_none() {
            return Ok(None);
        }

        let entry = storage_entry.unwrap();
        let mut user_entry: UserEntry = serde_json::from_slice(entry.value.as_slice())?;

        if user_entry.token_ttl.as_secs() == 0 && user_entry.ttl.as_secs() > 0 {
            user_entry.token_ttl = user_entry.ttl;
        }
        if user_entry.token_max_ttl.as_secs() == 0 && user_entry.max_ttl.as_secs() > 0 {
            user_entry.token_max_ttl = user_entry.max_ttl;
        }
        if user_entry.token_policies.is_empty() && !user_entry.policies.is_empty() {
            user_entry.token_policies = user_entry.policies.clone();
        }
        if user_entry.token_bound_cidrs.is_empty() && !user_entry.bound_cidrs.is_empty() {
            user_entry.token_bound_cidrs = user_entry.bound_cidrs.clone();
        }

        Ok(Some(user_entry))
    }

    pub async fn set_user(&self, req: &mut Request, name: &str, user_entry: &UserEntry) -> Result<(), RvError> {
        let entry = StorageEntry::new(format!("user/{name}").as_str(), user_entry)?;

        req.storage_put(&entry).await
    }

    pub async fn read_user(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let username_value = req.get_data("username")?;
        let username = username_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?.to_lowercase();

        let entry = self.get_user(req, &username).await?;
        if entry.is_none() {
            return Ok(None);
        }

        let user_entry = entry.unwrap();
        let mut user_entry_data = serde_json::to_value(&user_entry)?;
        let data = user_entry_data.as_object_mut().unwrap();
        data.remove("password_hash");

        user_entry.populate_token_data(data);

        if user_entry.ttl.as_secs() == 0 {
            data.remove("ttl");
        }

        if user_entry.max_ttl.as_secs() == 0 {
            data.remove("max_ttl");
        }

        if !user_entry.policies.is_empty() {
            data["policies"] = data["token_policies"].clone();
        }

        if !user_entry.bound_cidrs.is_empty() {
            data["bound_cidrs"] = data["token_bound_cidrs"].clone();
        }

        Ok(Some(Response::data_response(Some(data.clone()))))
    }

    pub async fn write_user(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let username_value = req.get_data("username")?;
        let username = username_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?.to_lowercase();

        let mut user_entry = UserEntry::default();

        if let Some(entry) = self.get_user(req, &username).await? {
            user_entry = entry;
        }

        if let Ok(password_value) = req.get_data("password") {
            let password = password_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
            if !password.is_empty() {
                user_entry.password_hash = self.gen_password_hash(password)?;
            }
        }

        let ttl_value = req.get_data_or_default("ttl")?;
        let ttl = ttl_value.as_int().ok_or(RvError::ErrRequestFieldInvalid)?;
        if ttl > 0 {
            user_entry.ttl = Duration::from_secs(ttl as u64);
        }

        let max_ttl_value = req.get_data_or_default("max_ttl")?;
        let max_ttl = max_ttl_value.as_u64().ok_or(RvError::ErrRequestFieldInvalid)?;
        if max_ttl > 0 {
            user_entry.max_ttl = Duration::from_secs(max_ttl);
        }

        let old_token_policies = user_entry.token_policies.clone();
        let old_token_ttl = user_entry.token_ttl;
        let old_token_max_ttl = user_entry.token_max_ttl;
        let old_token_bound_cidrs = user_entry.token_bound_cidrs.clone();

        user_entry.parse_token_fields(req)?;

        if old_token_policies != user_entry.token_policies {
            user_entry.policies = user_entry.token_policies.clone();
        } else if let Ok(policies_value) = req.get_data("policies") {
            let policies = policies_value.as_comma_string_slice().ok_or(RvError::ErrRequestFieldInvalid)?;
            user_entry.policies.clone_from(&policies);
            user_entry.token_policies = policies;
        }

        if old_token_ttl != user_entry.token_ttl {
            user_entry.ttl = user_entry.token_ttl;
        } else if let Ok(ttl_value) = req.get_data("ttl") {
            let ttl = ttl_value.as_duration().ok_or(RvError::ErrRequestFieldInvalid)?;
            user_entry.ttl = ttl;
            user_entry.token_ttl = ttl;
        }

        if old_token_max_ttl != user_entry.token_max_ttl {
            user_entry.max_ttl = user_entry.token_max_ttl;
        } else if let Ok(max_ttl_value) = req.get_data("max_ttl") {
            let max_ttl = max_ttl_value.as_duration().ok_or(RvError::ErrRequestFieldInvalid)?;
            user_entry.max_ttl = max_ttl;
            user_entry.token_max_ttl = max_ttl;
        }

        if old_token_bound_cidrs != user_entry.token_bound_cidrs {
            user_entry.bound_cidrs = user_entry.token_bound_cidrs.clone();
        } else if let Ok(bound_cidrs_value) = req.get_data("bound_cidrs") {
            let bound_cidrs = bound_cidrs_value.as_comma_string_slice().ok_or(RvError::ErrRequestFieldInvalid)?;
            user_entry.bound_cidrs = bound_cidrs
                .iter()
                .map(|s| SockAddrMarshaler::from_str(s))
                .collect::<Result<Vec<SockAddrMarshaler>, _>>()?;
            user_entry.token_bound_cidrs = user_entry.bound_cidrs.clone();
        }

        self.set_user(req, &username, &user_entry).await?;

        Ok(None)
    }

    pub async fn delete_user(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let username_value = req.get_data("username")?;
        let username = username_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        if username.is_empty() {
            return Err(RvError::ErrRequestNoDataField);
        }

        req.storage_delete(format!("user/{}", username.to_lowercase()).as_str()).await?;
        Ok(None)
    }

    pub async fn list_user(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let users = req.storage_list("user/").await?;
        let resp = Response::list_response(&users);
        Ok(Some(resp))
    }

    pub async fn write_user_password(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let username_value = req.get_data("username")?;
        let username = username_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        let mut user_entry = UserEntry::default();

        let entry = self.get_user(req, username).await?;
        if entry.is_some() {
            user_entry = entry.unwrap();
        }

        let password_value = req.get_data("password")?;
        let password = password_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        let password_hash = self.gen_password_hash(password)?;

        user_entry.password_hash = password_hash;

        self.set_user(req, username, &user_entry).await?;

        Ok(None)
    }

    pub fn gen_password_hash(&self, password: &str) -> Result<String, RvError> {
        let pwd_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)?;
        Ok(pwd_hash)
    }

    pub fn verify_password_hash(&self, password: &str, password_hash: &str) -> Result<bool, RvError> {
        let result = bcrypt::verify(password, password_hash)?;
        Ok(result)
    }
}
