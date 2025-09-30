use std::{collections::HashMap, mem, sync::Arc, time::SystemTime};

use super::{
    path_role::RoleEntry,
    validation::{create_hmac, verify_cidr_role_secret_id_subset},
    AppRoleBackend, AppRoleBackendInner,
};
use crate::{
    context::Context,
    errors::RvError,
    logical::{Auth, Backend, Field, FieldType, Operation, Path, PathOperation, Request, Response},
    new_fields, new_fields_internal, new_path, new_path_internal, rv_error_response, rv_error_string,
    storage::StorageEntry,
    utils::cidr,
};

impl AppRoleBackend {
    pub fn login_path(&self) -> Path {
        let approle_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"login$",
            fields: {
                "role_id": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Unique identifier of the Role. Required to be supplied when the 'bind_secret_id' constraint is set."
                },
                "secret_id": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "SecretID belong to the App role"
                }
            },
            operations: [
                {op: Operation::Write, handler: approle_backend_ref.login}
            ],
            help: r#"
While the credential 'role_id' is required at all times,
other credentials required depends on the properties App role
to which the 'role_id' belongs to. The 'bind_secret_id'
constraint (enabled by default) on the App role requires the
'secret_id' credential to be presented.

'role_id' is fetched using the 'role/<role_name>/role_id'
endpoint and 'secret_id' is fetched using the 'role/<role_name>/secret_id'
endpoint."#
        });

        path
    }
}

#[maybe_async::maybe_async]
impl AppRoleBackendInner {
    pub async fn login(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let role_id = req.get_data_as_str("role_id")?;

        let role_id_entry = self.get_role_id(req, &role_id).await?;
        if role_id_entry.is_none() {
            return Err(RvError::ErrResponse("invalid role_id".to_string()));
        }

        let role_id_entry = role_id_entry.unwrap();
        let role_name = role_id_entry.name.clone();

        let role_entry: RoleEntry;
        {
            let lock_entry = self.role_locks.get_lock(&role_name);
            let _locked = lock_entry.lock.read().await;

            role_entry = self
                .get_role(req, &role_id_entry.name)
                .await?
                .ok_or_else(|| RvError::ErrResponse("invalid role_id".to_string()))?;
        }

        let mut metadata: HashMap<String, String> = HashMap::new();

        let storage = Arc::as_ref(req.storage.as_ref().unwrap());

        if role_entry.bind_secret_id {
            let secret_id = req.get_data_as_str("secret_id")?;

            let secret_id_hmac = create_hmac(&role_entry.hmac_key, &secret_id)?;
            let role_name_hmac = create_hmac(&role_entry.hmac_key, &role_entry.name)?;

            let entry_index = format!("{}{}/{}", &role_entry.secret_id_prefix, &role_name_hmac, &secret_id_hmac);

            let lock_entry = self.secret_id_locks.get_lock(&secret_id_hmac);
            let lock = lock_entry.lock.clone();
            let locked = lock.read_owned().await;

            let secret_id_entry = self
                .get_secret_id_storage_entry(storage, &role_entry.secret_id_prefix, &role_name_hmac, &secret_id_hmac)
                .await?
                .ok_or(RvError::ErrResponse("invalid secret id".to_string()))?;

            // If a secret ID entry does not have a corresponding accessor entry, revoke the secret ID immediately
            let accessor_entry = self
                .get_secret_id_accessor_entry(
                    storage,
                    &secret_id_entry.secret_id_accessor,
                    &role_entry.secret_id_prefix,
                )
                .await?;
            if accessor_entry.is_none() {
                if let Err(err) = storage.delete(&entry_index).await {
                    return Err(RvError::ErrResponse(format!(
                        "error deleting secret_id {} from storage: {}",
                        &secret_id_hmac, err
                    )));
                }

                return Err(RvError::ErrResponse("invalid secret_id".to_string()));
            }

            if secret_id_entry.secret_id_num_uses == 0 {
                // secret_id_num_uses will be zero only if the usage limit was not set at all, in which case,
                // the secret_id will remain to be valid as long as it is not expired.

                // Ensure that the CIDRs on the secret id are still a subset of that of role's
                verify_cidr_role_secret_id_subset(&secret_id_entry.cidr_list, &role_entry.secret_id_bound_cidrs)?;

                if !secret_id_entry.cidr_list.is_empty() {
                    let conn = req
                        .connection
                        .as_ref()
                        .ok_or_else(|| RvError::ErrResponse("failed to get connection information".to_string()))?;
                    if conn.peer_addr.is_empty() {
                        return Err(RvError::ErrResponse("failed to get connection information".to_string()));
                    }

                    let cidr_list_ref: Vec<&str> = secret_id_entry.cidr_list.iter().map(AsRef::as_ref).collect();
                    if !cidr::ip_belongs_to_cidrs(&conn.peer_addr, &cidr_list_ref)? {
                        return Err(RvError::ErrResponse(format!(
                            "source address {} unauthorized through CIDR restrictions on the secret ID",
                            conn.peer_addr
                        )));
                    }
                }
            } else {
                // If the secret_id_num_uses is non-zero, it means that its use-count should be updated in the storage.
                // Switch the lock from a `read` to a `write` and update the storage entry.
                mem::drop(locked);
                let _locked = lock_entry.lock.write().await;

                // Lock switching may change the data. Refresh the contents.
                let mut secret_id_entry = self
                    .get_secret_id_storage_entry(
                        storage,
                        &role_entry.secret_id_prefix,
                        &role_name_hmac,
                        &secret_id_hmac,
                    )
                    .await?
                    .ok_or(RvError::ErrResponse("invalid secret id".to_string()))?;

                // If there exists a single use left, delete the secret_id entry from the storage but do not fail the
                // validation request. Subsequent requests to use the same secret_id will fail.
                if secret_id_entry.secret_id_num_uses == 1 {
                    // Delete the secret IDs accessor first
                    self.delete_secret_id_accessor_entry(
                        storage,
                        &secret_id_entry.secret_id_accessor,
                        &role_entry.secret_id_prefix,
                    )
                    .await?;

                    storage.delete(&entry_index).await?;
                } else {
                    secret_id_entry.secret_id_num_uses -= 1;
                    secret_id_entry.last_updated_time = SystemTime::now();
                    let entry = StorageEntry::new(&entry_index, &secret_id_entry)?;
                    storage.put(&entry).await?;
                }

                // Ensure that the CIDRs on the secret ID are still a subset of that of role's
                verify_cidr_role_secret_id_subset(&secret_id_entry.cidr_list, &role_entry.secret_id_bound_cidrs)?;

                if !secret_id_entry.cidr_list.is_empty() {
                    let conn = req
                        .connection
                        .as_ref()
                        .ok_or_else(|| RvError::ErrResponse("failed to get connection information".to_string()))?;
                    if conn.peer_addr.is_empty() {
                        return Err(RvError::ErrResponse("failed to get connection information".to_string()));
                    }

                    let cidr_list_ref: Vec<&str> = secret_id_entry.cidr_list.iter().map(AsRef::as_ref).collect();
                    if !cidr::ip_belongs_to_cidrs(&conn.peer_addr, &cidr_list_ref)? {
                        return Err(RvError::ErrResponse(format!(
                            "source address {} unauthorized through CIDR restrictions on the secret ID",
                            conn.peer_addr
                        )));
                    }
                }
            }

            metadata = secret_id_entry.metadata;
        }

        if !role_entry.secret_id_bound_cidrs.is_empty() {
            let conn = req
                .connection
                .as_ref()
                .ok_or_else(|| RvError::ErrResponse("failed to get connection information".to_string()))?;
            if conn.peer_addr.is_empty() {
                return Err(RvError::ErrResponse("failed to get connection information".to_string()));
            }

            let bound_cidrs_ref: Vec<&str> = role_entry.secret_id_bound_cidrs.iter().map(AsRef::as_ref).collect();
            if !cidr::ip_belongs_to_cidrs(&conn.peer_addr, &bound_cidrs_ref)? {
                return Err(RvError::ErrResponse(format!(
                    "source address {} unauthorized by CIDR restrictions on the secret ID",
                    conn.peer_addr
                )));
            }
        }

        metadata.insert("role_name".to_string(), role_entry.name.clone());

        let mut auth = Auth { metadata, ..Default::default() };
        auth.internal_data.insert("role_name".to_string(), role_entry.name.clone());

        role_entry.populate_token_auth(&mut auth);

        let resp = Response { auth: Some(auth), ..Response::default() };

        Ok(Some(resp))
    }

    pub async fn login_renew(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        if req.auth.is_none() {
            return Err(rv_error_string!("invalid request"));
        }
        let mut auth = req.auth.clone().unwrap();
        let role_name = auth.metadata.get("username");
        if role_name.is_none() {
            return Ok(None);
        }
        let role_name = role_name.unwrap();

        let role = self.get_role(req, role_name.as_str()).await?;
        if role.is_none() {
            return Ok(None);
        }

        let role = self
            .get_role(req, role_name)
            .await?
            .ok_or(rv_error_response!(format!("role {} does not exist during renewal", role_name)))?;

        auth.period = role.token_period;
        auth.ttl = role.token_ttl;
        auth.max_ttl = role.token_max_ttl;

        Ok(Some(Response { auth: Some(auth), ..Response::default() }))
    }
}
