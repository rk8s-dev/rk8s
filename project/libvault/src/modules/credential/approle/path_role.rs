use std::{collections::HashMap, mem, sync::Arc, time::Duration};

use better_default::Default;
use derive_more::{Deref, DerefMut};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{
    validation::{create_hmac, verify_cidr_role_secret_id_subset, SecretIdStorageEntry},
    AppRoleBackend, AppRoleBackendInner, HMAC_INPUT_LEN_MAX, SECRET_ID_LOCAL_PREFIX, SECRET_ID_PREFIX,
};
use crate::{
    context::Context,
    errors::RvError,
    logical::{field::FieldTrait, Backend, Field, FieldType, Operation, Path, PathOperation, Request, Response},
    new_fields, new_fields_internal, new_path, new_path_internal,
    storage::StorageEntry,
    utils::{
        self, deserialize_duration,
        policy::sanitize_policies,
        serialize_duration,
        sock_addr::SockAddrMarshaler,
        token_util::{token_fields, TokenParams},
    },
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Deref, DerefMut)]
pub struct RoleEntry {
    // Name of the role. This field is not persisted on disk. After the role is read out of disk,
    // the sanitized version of name is set in this field for subsequent use of role name
    // elsewhere.
    pub name: String,

    // UUID that uniquely represents this role. This serves as a credential to perform login using
    // this role.
    pub role_id: String,

    // UUID that serves as the HMAC key for the hashing the 'secret_id's of the role
    pub hmac_key: String,

    // Policies that are to be required by the token to access this role. Deprecated.
    pub policies: Vec<String>,

    // lower_case_role_name enforces the lower casing of role names for all the
    #[default(true)]
    pub lower_case_role_name: bool,

    // A constraint, if set, requires 'secret_id' credential to be presented during login
    pub bind_secret_id: bool,

    // Number of times the secret_id generated against this role can be used to perform login
    // operation
    pub secret_id_num_uses: i64,

    // SecretIDPrefix is the storage prefix for persisting secret IDs. This differs based on
    // whether the secret IDs are cluster local or not.
    pub secret_id_prefix: String,

    // Deprecated: A constraint, if set, specifies the CIDR blocks from which logins should be
    // allowed, please use secret_id_bound_cidrs instead.
    #[serde(rename = "bound_cidr_list", default)]
    pub bound_cidr_list_old: String,

    // Deprecated: A constraint, if set, specifies the CIDR blocks from which logins should be
    // allowed, please use secret_id_bound_cidrs instead.
    #[serde(rename = "bound_cidr_list_list", skip_serializing_if = "Vec::is_empty", default)]
    pub bound_cidr_list: Vec<String>,

    // A constraint, if set, specifies the CIDR blocks from which logins should be allowed
    pub secret_id_bound_cidrs: Vec<String>,

    // Duration (less than the backend mount's max TTL) after which a secret_id generated against
    // the role will expire
    #[serde(serialize_with = "serialize_duration", deserialize_with = "deserialize_duration")]
    pub secret_id_ttl: Duration,
    #[serde(serialize_with = "serialize_duration", deserialize_with = "deserialize_duration")]
    // Period, if set, indicates that the token generated using this role should never expire. The
    // token should be renewed within the duration specified by this value. The renewal duration
    // will be fixed if the value is not modified on the role. If the `Period` in the role is
    // modified, a token will pick up the new value during its next renewal. Deprecated.
    pub period: Duration,
    #[serde(flatten)]
    #[deref]
    #[deref_mut]
    pub token_params: TokenParams,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoleIdEntry {
    pub name: String,
}

impl RoleEntry {
    pub fn validate_role_constraints(&self) -> Result<(), RvError> {
        if self.bind_secret_id
            || !self.bound_cidr_list.is_empty()
            || !self.secret_id_bound_cidrs.is_empty()
            || !self.token_bound_cidrs.is_empty()
        {
            return Ok(());
        }

        Err(RvError::ErrResponse("at least one constraint should be enabled on the role".to_string()))
    }
}

impl AppRoleBackend {
    // role_path creates all the paths that are used to register and manage a role.
    //
    // role/ - For listing all the registered roles
    pub fn role_path(&self) -> Path {
        let approle_backend_ref = self.inner.clone();

        let mut path = new_path!({
            pattern: r"role/?",
            operations: [
                {op: Operation::List, handler: approle_backend_ref.list_role}
            ],
            help: "Lists all the roles registered with the backend."
        });

        path.fields.extend(token_fields());

        path
    }

    // role/<role_name> - For registering a role
    pub fn role_name_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let mut path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "bind_secret_id": {
                    field_type: FieldType::Bool,
                    default: true,
                    description: "Impose secret_id to be presented when logging in using this role. Defaults to 'true'."
                },
                "bound_cidr_list": {
                    field_type: FieldType::CommaStringSlice,
                    required: false,
                    description: r#"Use "secret_id_bound_cidrs" instead."#
                },
                "secret_id_bound_cidrs": {
                    field_type: FieldType::CommaStringSlice,
                    required: false,
                    description: r#"Comma separated string or list of CIDR blocks.
                    If set, specifies the blocks of IP addresses which can perform the login operation."#
                },
                "secret_id_num_uses": {
                    field_type: FieldType::Int,
                    required: false,
                    description: r#"Number of times a SecretID can access the role, after which the SecretID
        will expire. Defaults to 0 meaning that the the secret_id is of unlimited use."#
                },
                "secret_id_ttl": {
                    field_type: FieldType::DurationSecond,
                    required: false,
                    description: r#"Duration in seconds after which the issued SecretID should expire. Defaults to 0, meaning no expiration."#
                },
                "policies": {
                    field_type: FieldType::CommaStringSlice,
                    required: false,
                    description: "Use token_policies instead. If this and token_policies are both speicified, only token_policies will be used."
                },
                "period": {
                    field_type: FieldType::DurationSecond,
                    default: 0,
                    description: "Use token_period instead. If this and token_period are both speicified, only token_period will be used."
                },
                "role_id": {
                    field_type: FieldType::Str,
                    description: "Identifier of the role. Defaults to a UUID."
                },
                "local_secret_ids": {
                    field_type: FieldType::Bool,
                    default: false,
                    description: "If set, the secret IDs generated using this role will be cluster local. This can only be set during role creation and once set, it can't be reset later."
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role},
                {op: Operation::Write, handler: approle_backend_ref2.write_role},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role}
            ],
            help: r#"
A role can represent a service, a machine or anything that can be IDed.
The set of policies on the role defines access to the role, meaning, any
Vault token with a policy set that is a superset of the policies on the
role registered here will have access to the role. If a SecretID is desired
to be generated against only this specific role, it can be done via
'role/<role_name>/secret-id' and 'role/<role_name>/custom-secret-id' endpoints.
The properties of the SecretID created against the role and the properties
of the token issued with the SecretID generated against the role, can be
configured using the fields of this endpoint.
                "#
        });

        path.fields.extend(token_fields());

        path
    }

    // role/<role_name>/policies - For updating the param
    pub fn role_policies_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/policies$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "policies": {
                    field_type: FieldType::CommaStringSlice,
                    required: false,
                    description: "Use token_policies instead. If this and token_policies are both speicified, only token_policies will be used."
                },
                "token_policies": {
                    field_type: FieldType::CommaStringSlice,
                    required: true,
                    description: "Comma-separated list of policies"
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_policies},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_policies},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_policies}
            ],
            help: r#"
A comma-delimited set of Vault policies that defines access to the role.
All the Vault tokens with policies that encompass the policy set
defined on the role, can access the role.
                "#
        });

        path
    }

    // role/<role_name>/local-secret-ids - For reading the param
    pub fn role_local_secret_ids_path(&self) -> Path {
        let approle_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/local-secret-ids$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref.read_role_local_secret_ids}
            ],
            help: r#"If set, the secret IDs generated using this role will be cluster local.
This can only be set during role creation and once set, it can't be reset later.
                "#
        });

        path
    }

    // role/<role_name>/bound-cidr-list - For updating the param
    pub fn role_bound_cidr_list_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/bound-cidr-list$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "bound_cidr_list": {
                    field_type: FieldType::CommaStringSlice,
                    description: r#"Comma separated string or list of CIDR blocks.
        If set, specifies the blocks of IP addresses which can perform the login operation."#
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_bound_cidr_list},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_bound_cidr_list},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_bound_cidr_list}
            ],
            help: r#"
During login, the IP address of the client will be checked to see if it
belongs to the CIDR blocks specified. If CIDR blocks were set and if the
IP is not encompassed by it, login fails
                "#
        });

        path
    }

    // role/<role_name>/secret-id-bound-cidrs - For updating the param
    pub fn role_secret_id_bound_cidrs_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/secret-id-bound-cidrs$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "secret_id_bound_cidrs": {
                    field_type: FieldType::CommaStringSlice,
                    description: r#"Comma separated string or list of CIDR blocks.
        If set, specifies the blocks of IP addresses which can perform the login operation."#
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_secret_id_bound_cidrs},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_secret_id_bound_cidrs},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_secret_id_bound_cidrs}
            ],
            help: r#"
During login, the IP address of the client will be checked to see if it
belongs to the CIDR blocks specified. If CIDR blocks were set and if the
IP is not encompassed by it, login fails
                "#
        });

        path
    }

    // role/<role_name>/token-bound-cidrs - For updating the param
    pub fn role_token_bound_cidrs_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/token-bound-cidrs$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "token_bound_cidrs": {
                    field_type: FieldType::CommaStringSlice,
                    description: r#"Comma separated string or list of CIDR blocks. If set, specifies the blocks of IP addresses which can use the returned token. Should be a subset of the token CIDR blocks listed on the role, if any."#
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_token_bound_cidrs},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_token_bound_cidrs},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_token_bound_cidrs}
            ],
            help: r#"
During use of the returned token, the IP address of the client will be checked to see if it
belongs to the CIDR blocks specified. If CIDR blocks were set and if the
IP is not encompassed by it, token use fails
                "#
        });

        path
    }

    // role/<role_name>/bind-secret-id - For updating the param
    pub fn role_bind_secret_id_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/bind-secret-id$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "bind_secret_id": {
                    field_type: FieldType::Bool,
                    default: true,
                    description: "Impose secret_id to be presented when logging in using this role. Defaults to 'true'."
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_bind_secret_id},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_bind_secret_id},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_bind_secret_id}
            ],
            help: r#"
By setting this to 'true', during login the field 'secret_id' becomes a mandatory argument.
The value of 'secret_id' can be retrieved using 'role/<role_name>/secret-id' endpoint.
                "#
        });

        path
    }

    // role/<role_name>/secret-id-num-users - For updating the param
    pub fn role_secret_id_num_uses_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/secret-id-num-uses$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "secret_id_num_uses": {
                    field_type: FieldType::Int,
                    default: 0,
                    description: "Number of times a secret ID can access the role, after which the SecretID will expire. Defaults to 0 meaning that the secret ID is of unlimited use."
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_secret_id_num_uses},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_secret_id_num_uses},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_secret_id_num_uses}
            ],
            help: r#"
If a SecretID is generated/assigned against a role using the
'role/<role_name>/secret-id' or 'role/<role_name>/custom-secret-id' endpoint,
then the number of times this SecretID can be used is defined by this option.
However, this option may be overriden by the request's 'num_uses' field.
                "#
        });

        path
    }

    // role/<role_name>/secret-id-ttl - For updating the param
    pub fn role_secret_id_ttl_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/secret-id-ttl$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "secret_id_ttl": {
                    field_type: FieldType::Int,
                    default: 0,
                    description: "Duration in seconds after which the issued secret ID should expire. Defaults to 0, meaning no expiration."
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_secret_id_ttl},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_secret_id_ttl},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_secret_id_ttl}
            ],
            help: r#"
If a SecretID is generated/assigned against a role using the
'role/<role_name>/secret-id' or 'role/<role_name>/custom-secret-id' endpoint,
then the lifetime of this SecretID is defined by this option.
However, this option may be overridden by the request's 'ttl' field.
                "#
        });

        path
    }

    // role/<role_name>/period - For updating the param
    pub fn role_period_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/period$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "period": {
                    field_type: FieldType::DurationSecond,
                    default: 0,
                    description: "Use token_period instead. If this and token_period are both speicified, only token_period will be used."
                },
                "token_period": {
                    field_type: FieldType::DurationSecond,
                    description: "If set, tokens created via this role will have no max lifetime; instead, their renewal period will be fixed to this value."
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_period},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_period},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_period}
            ],
            help: r#"
If set,  indicates that the token generated using this role
should never expire. The token should be renewed within the
duration specified by this value. The renewal duration will
be fixed. If the Period in the role is modified, the token
will pick up the new value during its next renewal.
                "#
        });

        path
    }

    // role/<role_name>/token-num-uses - For updating the param
    pub fn role_token_num_uses_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/token-num-uses$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "token_num_uses": {
                    field_type: FieldType::Int,
                    default: 0,
                    description: "The maximum number of times a token may be used, a value of zero means unlimited"
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_token_num_uses},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_token_num_uses},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_token_num_uses}
            ],
            help: "By default, this will be set to zero, indicating that the issued"
        });

        path
    }

    // role/<role_name>/token-ttl - For updating the param
    pub fn role_token_ttl_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/token-ttl$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "token_ttl": {
                    field_type: FieldType::DurationSecond,
                    description: "The initial ttl of the token to generate"
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_token_ttl},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_token_ttl},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_token_ttl}
            ],
            help: r#"
If SecretIDs are generated against the role, using 'role/<role_name>/secret-id' or the
'role/<role_name>/custom-secret-id' endpoints, and if those SecretIDs are used
to perform the login operation, then the value of 'token-ttl' defines the
lifetime of the token issued, before which the token needs to be renewed.
                "#
        });

        path
    }

    // role/<role_name>/token-max-ttl - For updating the param
    pub fn role_token_max_ttl_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();
        let approle_backend_ref3 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/token-max-ttl$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "token_max_ttl": {
                    field_type: FieldType::DurationSecond,
                    description: "The maximum lifetime of the generated token"
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_token_max_ttl},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_token_max_ttl},
                {op: Operation::Delete, handler: approle_backend_ref3.delete_role_token_max_ttl}
            ],
            help: r#"
If SecretIDs are generated against the role using 'role/<role_name>/secret-id'
or the 'role/<role_name>/custom-secret-id' endpoints, and if those SecretIDs
are used to perform the login operation, then the value of 'token-max-ttl'
defines the maximum lifetime of the tokens issued, after which the tokens
cannot be renewed. A reauthentication is required after this duration.
This value will be capped by the backend mount's maximum TTL value.
                "#
        });

        path
    }

    // role/<role_name>/role-id - For updating the param
    pub fn role_role_id_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/role-id$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "role_id": {
                    field_type: FieldType::Str,
                    description: "Identifier of the role. Defaults to a UUID."
                }
            },
            operations: [
                {op: Operation::Read, handler: approle_backend_ref1.read_role_role_id},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_role_id}
            ],
            help: r#"
If login is performed from an role, then its 'role_id' should be presented
as a credential during the login. This 'role_id' can be retrieved using
this endpoint."#
        });

        path
    }

    // role/<role_name>/secret-id - For issuing a secret_id against a role, also to list the secret_id_accessors
    pub fn role_secret_id_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/secret-id/?$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "metadata": {
                    field_type: FieldType::Str,
                    description: r#"Metadata to be tied to the SecretID. This should be a JSON
        formatted string containing the metadata in key value pairs."#
                },
                "cidr_list": {
                    field_type: FieldType::CommaStringSlice,
                    description: r#"Comma separated string or list of CIDR blocks enforcing secret IDs to be used from
specific set of IP addresses. If 'bound_cidr_list' is set on the role, then the
list of CIDR blocks listed here should be a subset of the CIDR blocks listed on
the role."#
                },
                "token_bound_cidrs": {
                    field_type: FieldType::CommaStringSlice,
                    description: r#"List of CIDR blocks. If set, specifies the blocks of IP addresses which can use the returned token. Should be a subset of the token CIDR blocks listed on the role, if any."#
                },
                "num_uses": {
                    field_type: FieldType::Int,
                    description: r#"Number of times this SecretID can be used, after which the SecretID expires.
        Overrides secret_id_num_uses role option when supplied. May not be higher than role's secret_id_num_uses."#
                },
                "ttl": {
                    field_type: FieldType::DurationSecond,
                    description: r#"Duration in seconds after which this SecretID expires.
        Overrides secret_id_ttl role option when supplied. May not be longer than role's secret_id_ttl."#
                }
            },
            operations: [
                {op: Operation::List, handler: approle_backend_ref1.list_role_secret_id},
                {op: Operation::Write, handler: approle_backend_ref2.write_role_secret_id}
            ],
            help: r#"
The SecretID generated using this endpoint will be scoped to access
just this role and none else. The properties of this SecretID will be
based on the options set on the role. It will expire after a period
defined by the 'ttl' field or 'secret_id_ttl' option on the role,
and/or the backend mount's maximum TTL value."#
        });

        path
    }

    // role/<role_name>/secret-id/lookup - For reading the properties of a secret_id
    pub fn role_secret_id_lookup_path(&self) -> Path {
        let approle_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/secret-id/lookup/?$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "secret_id": {
                    field_type: FieldType::Str,
                    description: "SecretID attached to the role."
                }
            },
            operations: [
                {op: Operation::Write, handler: approle_backend_ref.write_role_secret_id_lookup}
            ],
            help: "This endpoint is used to read the properties of a secret_id associated to a role."
        });

        path
    }

    // role/<role_name>/secret-id/destroy - For deleting a secret_id
    pub fn role_secret_id_destroy_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/secret-id/destroy/?$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "secret_id": {
                    field_type: FieldType::Str,
                    description: "SecretID attached to the role."
                }
            },
            operations: [
                {op: Operation::Write, handler: approle_backend_ref1.write_role_secret_id_destory},
                {op: Operation::Delete, handler: approle_backend_ref2.delete_role_secret_id_destory}
            ],
            help: "This endpoint is used to delete the properties of a secret_id associated to a role."
        });

        path
    }

    // role/<role_name>/secret-id-accessor/lookup - For reading secret_id using accessor
    pub fn role_secret_id_accessor_lookup_path(&self) -> Path {
        let approle_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/secret-id-accessor/lookup/?$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "secret_id_accessor": {
                    field_type: FieldType::Str,
                    description: "Accessor of the SecretID"
                }
            },
            operations: [
                {op: Operation::Write, handler: approle_backend_ref.write_role_secret_id_accessor_lookup}
            ],
            help: r#"
This is particularly useful to lookup the non-expiring 'secret_id's.
The list operation on the 'role/<role_name>/secret-id' endpoint will return
the 'secret_id_accessor's. This endpoint can be used to read the properties
of the secret. If the 'secret_id_num_uses' field in the response is 0, it
represents a non-expiring 'secret_id'."#
        });

        path
    }

    // role/<role_name>/secret-id-accessor/destroy - For deleting secret_id using accessor
    pub fn role_secret_id_accessor_destroy_path(&self) -> Path {
        let approle_backend_ref1 = self.inner.clone();
        let approle_backend_ref2 = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/secret-id-accessor/destroy/?$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "secret_id_accessor": {
                    field_type: FieldType::Str,
                    description: "Accessor of the SecretID"
                }
            },
            operations: [
                {op: Operation::Write, handler: approle_backend_ref1.write_role_secret_id_accessor_destory},
                {op: Operation::Delete, handler: approle_backend_ref2.delete_role_secret_id_accessor_destory}
            ],
            help: r#"
This is particularly useful to clean-up the non-expiring 'secret_id's.
The list operation on the 'role/<role_name>/secret-id' endpoint will return
the 'secret_id_accessor's. This endpoint can be used to read the properties
of the secret. If the 'secret_id_num_uses' field in the response is 0, it
represents a non-expiring 'secret_id'."#
        });

        path
    }

    // role/<role_name>/custom-secret-id - For assigning a custom SecretID against a role
    pub fn role_custom_secret_id_path(&self) -> Path {
        let approle_backend_ref = self.inner.clone();

        let path = new_path!({
            pattern: r"role/(?P<role_name>\w[\w-]+\w)/custom-secret-id$",
            fields: {
                "role_name": {
                    field_type: FieldType::Str,
                    required: true,
                    description: "Name of the role."
                },
                "secret_id": {
                    field_type: FieldType::Str,
                    description: "SecretID to be attached to the role."
                },
                "metadata": {
                    field_type: FieldType::Str,
                    description: r#"Metadata to be tied to the SecretID. This should be a JSON
        formatted string containing the metadata in key value pairs."#
                },
                "cidr_list": {
                    field_type: FieldType::CommaStringSlice,
                    description: r#"Comma separated string or list of CIDR blocks enforcing secret IDs to be used from
specific set of IP addresses. If 'bound_cidr_list' is set on the role, then the
list of CIDR blocks listed here should be a subset of the CIDR blocks listed on
the role."#
                },
                "token_bound_cidrs": {
                    field_type: FieldType::CommaStringSlice,
                    description: r#"List of CIDR blocks. If set, specifies the blocks of IP addresses which can use the returned token. Should be a subset of the token CIDR blocks listed on the role, if any."#
                },
                "num_uses": {
                    field_type: FieldType::Int,
                    description: r#"Number of times this SecretID can be used, after which the SecretID expires.
        Overrides secret_id_num_uses role option when supplied. May not be higher than role's secret_id_num_uses."#
                },
                "ttl": {
                    field_type: FieldType::DurationSecond,
                    description: r#"Duration in seconds after which this SecretID expires.
        Overrides secret_id_ttl role option when supplied. May not be longer than role's secret_id_ttl."#
                }
            },
            operations: [
                {op: Operation::Write, handler: approle_backend_ref.write_role_custom_secret_id}
            ],
            help: r#"
This option is not recommended unless there is a specific need
to do so. This will assign a client supplied SecretID to be used to access
the role. This SecretID will behave similarly to the SecretIDs generated by
the backend. The properties of this SecretID will be based on the options
set on the role. It will expire after a period defined by the 'ttl' field
or 'secret_id_ttl' option on the role, and/or the backend mount's maximum TTL value."#
        });

        path
    }

    pub fn role_paths(&self) -> Vec<Path> {
        let paths: Vec<Path> = vec![
            self.role_path(),
            self.role_name_path(),
            self.role_policies_path(),
            self.role_local_secret_ids_path(),
            self.role_bound_cidr_list_path(),
            self.role_secret_id_bound_cidrs_path(),
            self.role_token_bound_cidrs_path(),
            self.role_bind_secret_id_path(),
            self.role_secret_id_num_uses_path(),
            self.role_secret_id_ttl_path(),
            self.role_period_path(),
            self.role_token_num_uses_path(),
            self.role_token_ttl_path(),
            self.role_token_max_ttl_path(),
            self.role_role_id_path(),
            self.role_secret_id_path(),
            self.role_secret_id_lookup_path(),
            self.role_secret_id_destroy_path(),
            self.role_secret_id_accessor_lookup_path(),
            self.role_secret_id_accessor_destroy_path(),
            self.role_custom_secret_id_path(),
        ];
        paths
    }
}

#[allow(clippy::assigning_clones)]
#[maybe_async::maybe_async]
impl AppRoleBackendInner {
    pub async fn get_role_id(&self, req: &mut Request, role_id: &str) -> Result<Option<RoleIdEntry>, RvError> {
        if role_id.is_empty() {
            return Err(RvError::ErrResponse("missing role_id".to_string()));
        }

        let salt = self.salt.load();
        if salt.is_none() {
            return Err(RvError::ErrResponse("salt not found".to_string()));
        }

        let salt_id = salt.as_ref().unwrap().salt_id(role_id)?;
        let storage_entry = req.storage_get(format!("role_id/{salt_id}").as_str()).await?;
        if storage_entry.is_none() {
            return Ok(None);
        }

        let entry = storage_entry.unwrap();
        let role_id_entry: RoleIdEntry = serde_json::from_slice(entry.value.as_slice())?;

        Ok(Some(role_id_entry))
    }

    pub async fn set_role_id(
        &self,
        req: &mut Request,
        role_id: &str,
        role_id_entry: &RoleIdEntry,
    ) -> Result<(), RvError> {
        let Some(salt) = self.salt.load_full() else {
            return Err(RvError::ErrResponse("salt not found".to_string()));
        };

        let salt_id = salt.salt_id(role_id)?;

        let entry = StorageEntry::new(format!("role_id/{salt_id}").as_str(), role_id_entry)?;

        req.storage_put(&entry).await
    }

    pub async fn delete_role_id(&self, req: &mut Request, role_id: &str) -> Result<(), RvError> {
        if role_id.is_empty() {
            return Err(RvError::ErrResponse("missing role_id".to_string()));
        }

        let Some(salt) = self.salt.load_full() else {
            return Err(RvError::ErrResponse("salt not found".to_string()));
        };

        let salt_id = salt.salt_id(role_id)?;

        req.storage_delete(format!("role_id/{salt_id}").as_str()).await?;

        Ok(())
    }

    pub async fn get_role(&self, req: &mut Request, name: &str) -> Result<Option<RoleEntry>, RvError> {
        let key = format!("role/{}", name.to_lowercase());
        let storage_entry = req.storage_get(&key).await?;
        if storage_entry.is_none() {
            return Ok(None);
        }

        let entry = storage_entry.unwrap();
        let mut role_entry: RoleEntry = serde_json::from_slice(entry.value.as_slice())?;

        role_entry.name = name.to_string();
        if role_entry.lower_case_role_name {
            role_entry.name = name.to_lowercase();
        }

        if role_entry.secret_id_prefix.is_empty() {
            role_entry.secret_id_prefix = SECRET_ID_PREFIX.to_string();
        }

        if !role_entry.bound_cidr_list_old.is_empty() {
            role_entry.secret_id_bound_cidrs =
                role_entry.bound_cidr_list_old.split(',').map(|s| s.to_string()).collect();
            role_entry.bound_cidr_list_old.clear();
        }

        if !role_entry.bound_cidr_list.is_empty() {
            role_entry.secret_id_bound_cidrs.clone_from(&role_entry.bound_cidr_list);
            role_entry.bound_cidr_list.clear();
        }

        if role_entry.token_period.as_secs() == 0 && role_entry.period.as_secs() > 0 {
            role_entry.token_period = role_entry.period;
        }

        if role_entry.token_policies.is_empty() && !role_entry.policies.is_empty() {
            role_entry.token_policies = role_entry.policies.clone();
        }

        Ok(Some(role_entry))
    }

    pub async fn set_role(
        &self,
        req: &mut Request,
        name: &str,
        role_entry: &RoleEntry,
        previous_role_id: &str,
    ) -> Result<(), RvError> {
        if name.is_empty() {
            return Err(RvError::ErrResponse("missing role name".to_string()));
        }

        role_entry.validate_role_constraints()?;

        if let Some(role_id_entry) = self.get_role_id(req, &role_entry.role_id).await? {
            if role_id_entry.name.as_str() != name {
                return Err(RvError::ErrResponse("role_id already in use".to_string()));
            }
        }

        let mut create_role_id = true;

        if !previous_role_id.is_empty() {
            if previous_role_id != role_entry.role_id.as_str() {
                self.delete_role_id(req, previous_role_id).await?;
            } else {
                create_role_id = false;
            }
        }

        let entry = StorageEntry::new(format!("role/{}", name.to_lowercase()).as_str(), role_entry)?;

        req.storage_put(&entry).await?;

        if create_role_id {
            return self.set_role_id(req, &role_entry.role_id, &RoleIdEntry { name: name.to_string() }).await;
        }

        Ok(())
    }

    pub async fn list_role(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let roles = req.storage_list("role/").await?;
        Ok(Some(Response::list_response(&roles)))
    }

    pub async fn write_role(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let role_name_value = req.get_data("role_name")?;
        let role_name = role_name_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        if role_name.len() > HMAC_INPUT_LEN_MAX {
            return Err(RvError::ErrResponse(
                format!("role_name is longer than maximum of {HMAC_INPUT_LEN_MAX} bytes").to_string(),
            ));
        }

        let mut role_entry = RoleEntry::default();
        let mut create = false;

        let lock_entry = self.role_locks.get_lock(role_name);
        let _locked = lock_entry.lock.write().await;

        let entry = self.get_role(req, role_name).await?;
        if entry.is_some() {
            role_entry = entry.unwrap();
        } else {
            role_entry.name = role_name.to_lowercase();
            role_entry.lower_case_role_name = true;
            role_entry.hmac_key = utils::generate_uuid();
            create = true;
        }

        let old_token_policies = role_entry.token_policies.clone();
        let old_token_period = role_entry.token_period;

        role_entry.parse_token_fields(req)?;

        if old_token_policies != role_entry.token_policies {
            role_entry.policies = role_entry.token_policies.clone();
        } else if let Ok(policies_value) = req.get_data("policies") {
            let policies = policies_value.as_comma_string_slice().ok_or(RvError::ErrRequestFieldInvalid)?;
            role_entry.policies.clone_from(&policies);
            role_entry.token_policies = policies;
        }

        if old_token_period != role_entry.token_period {
            role_entry.period = role_entry.token_period;
        } else if let Ok(period_value) = req.get_data("period") {
            let period = period_value.as_duration().ok_or(RvError::ErrRequestFieldInvalid)?;
            role_entry.period = period;
            role_entry.token_period = period;
        }

        if let Ok(local_secret_ids_value) = req.get_data("local_secret_ids") {
            let local_secret_ids = local_secret_ids_value.as_bool().ok_or(RvError::ErrRequestFieldInvalid)?;
            if local_secret_ids {
                if !create {
                    return Err(RvError::ErrResponse(
                        "local_secret_ids can only be modified during role creation".to_string(),
                    ));
                }
                role_entry.secret_id_prefix = SECRET_ID_LOCAL_PREFIX.to_string();
            }
        }

        let previous_role_id = role_entry.role_id.clone();

        if let Ok(role_id_value) = req.get_data("role_id") {
            role_entry.role_id = role_id_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?.to_string();
        } else if create {
            role_entry.role_id = utils::generate_uuid();
        }

        if role_entry.role_id.is_empty() {
            return Err(RvError::ErrResponse("invalid role_id supplied, or failed to generate a role_id".to_string()));
        }

        if let Ok(bind_secret_id_value) = req.get_data("bind_secret_id") {
            role_entry.bind_secret_id = bind_secret_id_value.as_bool().ok_or(RvError::ErrRequestFieldInvalid)?;
        } else if create {
            role_entry.bind_secret_id =
                req.get_data_or_default("bind_secret_id")?.as_bool().ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(bound_cidr_list_value) = req.get_data_or_next(&["secret_id_bound_cidrs", "bound_cidr_list"]) {
            role_entry.secret_id_bound_cidrs =
                bound_cidr_list_value.as_comma_string_slice().ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if !role_entry.secret_id_bound_cidrs.is_empty() {
            let cidrs: Vec<&str> = role_entry.secret_id_bound_cidrs.iter().map(AsRef::as_ref).collect();
            if !utils::cidr::validate_cidrs(&cidrs)? {
                return Err(RvError::ErrResponse("invalid CIDR blocks".to_string()));
            }
        }

        if let Ok(secret_id_num_uses_value) = req.get_data("secret_id_num_uses") {
            role_entry.secret_id_num_uses = secret_id_num_uses_value.as_int().ok_or(RvError::ErrRequestFieldInvalid)?;
        } else if create {
            role_entry.secret_id_num_uses =
                req.get_data_or_default("secret_id_num_uses")?.as_int().ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if role_entry.secret_id_num_uses < 0 {
            return Err(RvError::ErrResponse("secret_id_num_uses cannot be negative".to_string()));
        }

        if let Ok(secret_id_ttl_value) = req.get_data("secret_id_ttl") {
            role_entry.secret_id_ttl = secret_id_ttl_value.as_duration().ok_or(RvError::ErrRequestFieldInvalid)?;
        } else if create {
            role_entry.secret_id_ttl =
                req.get_data_or_default("secret_id_ttl")?.as_duration().ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        self.set_role(req, &role_entry.name, &role_entry, &previous_role_id).await?;

        Ok(None)
    }

    pub async fn read_role(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let locked = lock_entry.lock.read().await;

        if let Some(entry) = self.get_role(req, &role_name).await? {
            let mut data = serde_json::json!({
                "bind_secret_id": entry.bind_secret_id,
                "secret_id_bound_cidrs": entry.secret_id_bound_cidrs,
                "secret_id_num_uses": entry.secret_id_num_uses,
                "secret_id_ttl": entry.secret_id_ttl.as_secs(),
                "local_secret_ids": false,
            })
            .as_object()
            .unwrap()
            .clone();

            if entry.secret_id_prefix.as_str() == SECRET_ID_LOCAL_PREFIX {
                data["local_secret_ids"] = Value::from(true);
            }

            if entry.period.as_secs() != 0 {
                data.insert("period".to_string(), Value::from(entry.period.as_secs()));
            }

            if !entry.policies.is_empty() {
                data.insert("policies".to_string(), Value::from(entry.policies.clone()));
            }

            entry.populate_token_data(&mut data);

            if entry.validate_role_constraints().is_err() {
                log::warn!(
                    "Role does not have any constraints set on it. Updates to this role will require a constraint to \
                     be set"
                );
            }

            let mut resp = Response::data_response(Some(data));

            // For sanity, verify that the index still exists. If the index is missing,
            // add one and return a warning so it can be reported.
            if self.get_role_id(req, &entry.role_id).await?.is_none() {
                // Switch to a write lock
                mem::drop(locked);
                let _locked = lock_entry.lock.write().await;

                // Check again if the index is missing
                if self.get_role_id(req, &entry.role_id).await?.is_none() {
                    // Create a new inde
                    self.set_role_id(req, &entry.role_id, &RoleIdEntry { name: entry.name.clone() }).await?;
                    resp.add_warning("Role identifier was missing an index back to role name");
                }
            }

            return Ok(Some(resp));
        }

        Ok(None)
    }

    pub async fn delete_role(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.write().await;

        if let Some(entry) = self.get_role(req, &role_name).await? {
            let storage = req.storage.as_ref().unwrap();

            self.flush_role_secrets(Arc::as_ref(storage), &entry.name, &entry.hmac_key, &entry.secret_id_prefix)
                .await?;

            self.delete_role_id(req, &entry.role_id).await?;

            req.storage_delete(format!("role/{}", role_name.to_lowercase()).as_str()).await?;
        }

        Ok(None)
    }

    pub async fn read_role_policies(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.read().await;

        if let Some(role) = self.get_role(req, &role_name).await? {
            let mut data = serde_json::json!({
                "token_policies": role.token_policies,
            })
            .as_object()
            .unwrap()
            .clone();

            if !role.policies.is_empty() {
                data.insert("policies".to_string(), Value::from(role.policies));
            }

            Ok(Some(Response::data_response(Some(data))))
        } else {
            Ok(None)
        }
    }

    pub async fn write_role_policies(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;

        let token_policies_value = req.get_data_or_next(&["token_policies", "policies"])?;
        let mut token_policies = token_policies_value.as_comma_string_slice().ok_or(RvError::ErrRequestFieldInvalid)?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.write().await;

        if let Some(mut role) = self.get_role(req, &role_name).await? {
            sanitize_policies(&mut token_policies, false);
            role.policies.clone_from(&token_policies);
            role.token_policies = token_policies;
            self.set_role(req, &role_name, &role, "").await?;
        } else {
            return Err(RvError::ErrLogicalPathUnsupported);
        }

        Ok(None)
    }

    pub async fn delete_role_policies(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.write().await;

        if let Some(mut role) = self.get_role(req, &role_name).await? {
            role.token_policies.clear();
            role.policies.clear();
            self.set_role(req, &role_name, &role, "").await?;
        } else {
            return Err(RvError::ErrLogicalPathUnsupported);
        }

        Ok(None)
    }

    pub async fn read_role_local_secret_ids(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "local_secret_ids").await
    }

    pub async fn read_role_field(&self, req: &mut Request, field: &str) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.read().await;

        if let Some(role) = self.get_role(req, &role_name).await? {
            let data = match field {
                "bound_cidr_list" => {
                    serde_json::json!({
                        "bound_cidr_list": role.bound_cidr_list,
                    })
                }
                "secret_id_bound_cidrs" => {
                    serde_json::json!({
                        "secret_id_bound_cidrs": role.secret_id_bound_cidrs,
                    })
                }
                "token_bound_cidrs" => {
                    serde_json::json!({
                        "token_bound_cidrs": role.token_bound_cidrs,
                    })
                }
                "bind_secret_id" => {
                    serde_json::json!({
                        "bind_secret_id": role.bind_secret_id,
                    })
                }
                "local_secret_ids" => {
                    serde_json::json!({
                        "local_secret_ids": role.secret_id_prefix.as_str() == SECRET_ID_LOCAL_PREFIX,
                    })
                }
                "secret_id_num_uses" => {
                    serde_json::json!({
                        "secret_id_num_uses": role.secret_id_num_uses,
                    })
                }
                "role_id" => {
                    serde_json::json!({
                        "role_id": role.role_id,
                    })
                }
                "secret_id_ttl" => {
                    serde_json::json!({
                        "secret_id_ttl": role.secret_id_ttl.as_secs(),
                    })
                }
                "token_period" | "period" => {
                    if role.period.as_secs() > 0 {
                        serde_json::json!({
                            "token_period": role.token_period.as_secs(),
                            "period": role.period.as_secs(),
                        })
                    } else {
                        serde_json::json!({
                            "token_period": role.token_period.as_secs(),
                        })
                    }
                }
                "token_num_uses" => {
                    serde_json::json!({
                        "token_num_uses": role.token_num_uses,
                    })
                }
                "token_ttl" => {
                    serde_json::json!({
                        "token_ttl": role.token_ttl.as_secs(),
                    })
                }
                "token_max_ttl" => {
                    serde_json::json!({
                        "token_max_ttl": role.token_max_ttl.as_secs(),
                    })
                }
                _ => {
                    return Err(RvError::ErrResponse("unrecognized field".to_string()));
                }
            };
            Ok(Some(Response::data_response(Some(data.as_object().unwrap().clone()))))
        } else {
            Ok(None)
        }
    }

    pub async fn update_role_field(&self, req: &mut Request, field: &str) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;

        let field_value = match field {
            "token_period" | "period" => req.get_data_or_next(&["token_period", "period"])?,
            _ => req.get_data(field)?,
        };

        let mut cidr_list = Vec::new();

        match field {
            "bound_cidr_list" | "secret_id_bound_cidrs" | "token_bound_cidrs" => {
                cidr_list = field_value.as_comma_string_slice().ok_or(RvError::ErrRequestFieldInvalid)?;
                if cidr_list.is_empty() {
                    return Err(RvError::ErrResponse(format!("missing {field}").to_string()));
                }

                let cidrs: Vec<&str> = cidr_list.iter().map(AsRef::as_ref).collect();
                if !utils::cidr::validate_cidrs(&cidrs)? {
                    return Err(RvError::ErrResponse("failed to validate CIDR blocks".to_string()));
                }
            }
            _ => {}
        }

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.write().await;

        let mut previous_role_id = "".to_string();

        if let Some(mut role) = self.get_role(req, &role_name).await? {
            match field {
                "bound_cidr_list" | "secret_id_bound_cidrs" => {
                    role.secret_id_bound_cidrs = cidr_list;
                }
                "token_bound_cidrs" => {
                    role.token_bound_cidrs = cidr_list
                        .iter()
                        .map(|s| SockAddrMarshaler::from_str(s))
                        .collect::<Result<Vec<SockAddrMarshaler>, _>>()?;
                }
                "bind_secret_id" => {
                    role.bind_secret_id = field_value.as_bool().ok_or(RvError::ErrLogicalOperationUnsupported)?;
                }
                "secret_id_num_uses" => {
                    role.secret_id_num_uses = field_value.as_int().ok_or(RvError::ErrLogicalOperationUnsupported)?;
                    if role.secret_id_num_uses < 0 {
                        return Err(RvError::ErrResponse("secret_id_num_uses cannot be negative".to_string()));
                    }
                }
                "role_id" => {
                    previous_role_id.clone_from(&role.role_id);
                    role.role_id = field_value.as_str().ok_or(RvError::ErrLogicalOperationUnsupported)?.to_string();
                    if role.role_id.as_str() == "" {
                        return Err(RvError::ErrResponse("missing role_id".to_string()));
                    }
                }
                "secret_id_ttl" => {
                    role.secret_id_ttl = field_value.as_duration().ok_or(RvError::ErrLogicalOperationUnsupported)?;
                }
                "token_period" | "period" => {
                    role.token_period = field_value.as_duration().ok_or(RvError::ErrLogicalOperationUnsupported)?;
                    role.period = role.token_period;
                }
                "token_num_uses" => {
                    role.token_num_uses = field_value.as_u64().ok_or(RvError::ErrLogicalOperationUnsupported)?;
                }
                "token_ttl" => {
                    role.token_ttl = field_value.as_duration().ok_or(RvError::ErrLogicalOperationUnsupported)?;
                    if role.token_max_ttl.as_secs() > 0 && role.token_ttl.as_secs() > role.token_max_ttl.as_secs() {
                        return Err(RvError::ErrResponse(
                            "token_ttl should not be greater than token_max_ttl".to_string(),
                        ));
                    }
                }
                "token_max_ttl" => {
                    role.token_max_ttl = field_value.as_duration().ok_or(RvError::ErrLogicalOperationUnsupported)?;
                    if role.token_max_ttl.as_secs() > 0 && role.token_ttl.as_secs() > role.token_max_ttl.as_secs() {
                        return Err(RvError::ErrResponse(
                            "token_max_ttl should not be greater than token_ttl".to_string(),
                        ));
                    }
                }
                _ => {
                    return Err(RvError::ErrResponse("unrecognized field".to_string()));
                }
            }

            self.set_role(req, &role_name, &role, &previous_role_id).await?;
        } else {
            return Err(RvError::ErrLogicalPathUnsupported);
        }

        Ok(None)
    }

    pub async fn delete_role_field(&self, req: &mut Request, field: &str) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.write().await;

        if let Some(mut role) = self.get_role(req, &role_name).await? {
            match field {
                "bound_cidr_list" => {
                    role.bound_cidr_list.clear();
                }
                "secret_id_bound_cidrs" => {
                    role.secret_id_bound_cidrs.clear();
                }
                "token_bound_cidrs" => {
                    role.token_bound_cidrs.clear();
                }
                "bind_secret_id" => {
                    role.bind_secret_id = req
                        .get_field_default_or_zero("bind_secret_id")?
                        .as_bool()
                        .ok_or(RvError::ErrLogicalOperationUnsupported)?;
                }
                "secret_id_num_uses" => {
                    role.secret_id_num_uses = req
                        .get_field_default_or_zero("secret_id_num_uses")?
                        .as_int()
                        .ok_or(RvError::ErrLogicalOperationUnsupported)?;
                }
                "secret_id_ttl" => {
                    role.secret_id_ttl = req
                        .get_field_default_or_zero("secret_id_ttl")?
                        .as_duration()
                        .ok_or(RvError::ErrLogicalOperationUnsupported)?;
                }
                "token_period" | "period" => {
                    role.token_period = Duration::from_secs(0);
                    role.period = Duration::from_secs(0);
                }
                "token_num_uses" => {
                    role.token_num_uses = req
                        .get_field_default_or_zero("token_num_uses")?
                        .as_u64()
                        .ok_or(RvError::ErrLogicalOperationUnsupported)?;
                }
                "token_ttl" => {
                    role.token_ttl = req
                        .get_field_default_or_zero("token_ttl")?
                        .as_duration()
                        .ok_or(RvError::ErrLogicalOperationUnsupported)?;
                }
                "token_max_ttl" => {
                    role.token_max_ttl = req
                        .get_field_default_or_zero("token_max_ttl")?
                        .as_duration()
                        .ok_or(RvError::ErrLogicalOperationUnsupported)?;
                }
                _ => {
                    return Err(RvError::ErrResponse("unrecognized field".to_string()));
                }
            }

            self.set_role(req, &role_name, &role, "").await?;
        }

        Ok(None)
    }

    pub async fn read_role_bound_cidr_list(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "bound_cidr_list").await
    }

    pub async fn write_role_bound_cidr_list(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "bound_cidr_list").await
    }

    pub async fn delete_role_bound_cidr_list(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_field(req, "bound_cidr_list").await
    }

    pub async fn read_role_secret_id_bound_cidrs(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "secret_id_bound_cidrs").await
    }

    pub async fn write_role_secret_id_bound_cidrs(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "secret_id_bound_cidrs").await
    }

    pub async fn delete_role_secret_id_bound_cidrs(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_field(req, "secret_id_bound_cidrs").await
    }

    pub async fn read_role_token_bound_cidrs(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "token_bound_cidrs").await
    }

    pub async fn write_role_token_bound_cidrs(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "token_bound_cidrs").await
    }

    pub async fn delete_role_token_bound_cidrs(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_field(req, "token_bound_cidrs").await
    }

    pub async fn read_role_bind_secret_id(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "bind_secret_id").await
    }

    pub async fn write_role_bind_secret_id(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "bind_secret_id").await
    }

    pub async fn delete_role_bind_secret_id(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_field(req, "bind_secret_id").await
    }

    pub async fn read_role_secret_id_num_uses(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "secret_id_num_uses").await
    }

    pub async fn write_role_secret_id_num_uses(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "secret_id_num_uses").await
    }

    pub async fn delete_role_secret_id_num_uses(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_field(req, "secret_id_num_uses").await
    }

    pub async fn read_role_secret_id_ttl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "secret_id_ttl").await
    }

    pub async fn write_role_secret_id_ttl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "secret_id_ttl").await
    }

    pub async fn delete_role_secret_id_ttl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_field(req, "secret_id_ttl").await
    }

    pub async fn read_role_period(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "token_period").await
    }

    pub async fn write_role_period(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "token_period").await
    }

    pub async fn delete_role_period(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_field(req, "token_period").await
    }

    pub async fn read_role_token_num_uses(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "token_num_uses").await
    }

    pub async fn write_role_token_num_uses(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "token_num_uses").await
    }

    pub async fn delete_role_token_num_uses(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_field(req, "token_num_uses").await
    }

    pub async fn read_role_token_ttl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "token_ttl").await
    }

    pub async fn write_role_token_ttl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "token_ttl").await
    }

    pub async fn delete_role_token_ttl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_field(req, "token_ttl").await
    }

    pub async fn read_role_token_max_ttl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "token_max_ttl").await
    }

    pub async fn write_role_token_max_ttl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "token_max_ttl").await
    }

    pub async fn delete_role_token_max_ttl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_field(req, "token_max_ttl").await
    }

    pub async fn read_role_role_id(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.read_role_field(req, "role_id").await
    }

    pub async fn write_role_role_id(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_field(req, "role_id").await
    }

    pub async fn list_role_secret_id(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.read().await;

        if let Some(role) = self.get_role(req, &role_name).await? {
            let role_name_hmac = create_hmac(&role.hmac_key, &role.name)?;
            let key = format!("{}{}/", role.secret_id_prefix, role_name_hmac);
            let secret_id_hmacs = req.storage_list(&key).await?;

            let mut list_items: Vec<String> = Vec::with_capacity(secret_id_hmacs.len());
            for secret_id_hmac in secret_id_hmacs.iter() {
                let entry_index = format!("{}{}/{}", role.secret_id_prefix, role_name_hmac, secret_id_hmac);

                // secret_id locks are not indexed by secret_id itself.
                // This is because secret_id are not stored in plaintext
                // form anywhere in the backend, and hence accessing its
                // corresponding lock many times using secret_id is not
                // possible. Also, indexing it everywhere using secret_id_hmacs
                // makes listing operation easier.
                let lock_entry = self.secret_id_locks.get_lock(secret_id_hmac);
                let _locked = lock_entry.lock.read().await;
                let storage_entry = req.storage_get(&entry_index).await?;
                if storage_entry.is_none() {
                    return Err(RvError::ErrResponse(
                        "storage entry for SecretID is present but no content found at the index".to_string(),
                    ));
                }
                let entry = storage_entry.unwrap();
                let secret_id_entry: SecretIdStorageEntry = serde_json::from_slice(entry.value.as_slice())?;
                list_items.push(secret_id_entry.secret_id_accessor);
            }

            return Ok(Some(Response::list_response(&list_items)));
        }

        Err(RvError::ErrResponse(format!("role {role_name} does not exist")))
    }

    pub async fn update_role_secret_id_common(
        &self,
        req: &mut Request,
        secret_id: &str,
    ) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;

        if secret_id.is_empty() {
            return Err(RvError::ErrResponse("missing secret_id".to_string()));
        }

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.read().await;

        let role = self.get_role(req, &role_name).await?;
        if role.is_none() {
            return Err(RvError::ErrResponse(format!("role {role_name} does not exist")));
        }

        let role = role.unwrap();

        if !role.bind_secret_id {
            return Err(RvError::ErrResponse("bind_secret_id is not set on the role".to_string()));
        }

        let cidr_list_value = req.get_data_or_default("cidr_list")?;
        let cidr_list = cidr_list_value.as_comma_string_slice().ok_or(RvError::ErrRequestFieldInvalid)?;
        // Validate the list of CIDR blocks
        if !cidr_list.is_empty() {
            let cidrs: Vec<&str> = cidr_list.iter().map(AsRef::as_ref).collect();
            if !utils::cidr::validate_cidrs(&cidrs)? {
                return Err(RvError::ErrResponse("failed to validate CIDR blocks".to_string()));
            }
        }

        // Ensure that the CIDRs on the secret ID are a subset of that of role's
        verify_cidr_role_secret_id_subset(&cidr_list, &role.secret_id_bound_cidrs)?;

        let token_bound_cidrs_value = req.get_data_or_default("token_bound_cidrs")?;
        let token_bound_cidrs =
            token_bound_cidrs_value.as_comma_string_slice().ok_or(RvError::ErrRequestFieldInvalid)?;
        // Validate the list of CIDR blocks
        if !token_bound_cidrs.is_empty() {
            let cidrs: Vec<&str> = token_bound_cidrs.iter().map(AsRef::as_ref).collect();
            if !utils::cidr::validate_cidrs(&cidrs)? {
                return Err(RvError::ErrResponse("failed to validate CIDR blocks".to_string()));
            }
        }

        // Ensure that the token CIDRs on the secret ID are a subset of that of role's
        let role_token_bound_cidrs =
            role.token_bound_cidrs.iter().map(|s| s.sock_addr.to_string()).collect::<Vec<String>>();
        verify_cidr_role_secret_id_subset(&token_bound_cidrs, &role_token_bound_cidrs)?;

        // Check whether or not specified num_uses is defined, otherwise fallback to role's secret_id_num_uses
        let num_uses: i64;
        if let Ok(num_uses_value) = req.get_data("num_uses") {
            num_uses = num_uses_value.as_i64().ok_or(RvError::ErrRequestFieldInvalid)?;
            if num_uses < 0 {
                return Err(RvError::ErrResponse("num_uses cannot be negative".to_string()));
            }
            // If the specified num_uses is higher than the role's secret_id_num_uses, throw an error rather than implicitly overriding
            if role.secret_id_num_uses > 0 && (num_uses == 0 || num_uses > role.secret_id_num_uses) {
                return Err(RvError::ErrResponse(
                    "num_uses cannot be higher than the role's secret_id_num_uses".to_string(),
                ));
            }
        } else {
            num_uses = role.secret_id_num_uses;
        }

        // Check whether or not specified ttl is defined, otherwise fallback to role's secret_id_ttl
        let ttl: Duration;
        if let Ok(ttl_value) = req.get_data("ttl") {
            ttl = ttl_value.as_duration().ok_or(RvError::ErrRequestFieldInvalid)?;
            if (ttl.as_secs() == 0 && role.secret_id_ttl.as_secs() > 0)
                || (role.secret_id_ttl.as_secs() > 0 && ttl.as_secs() > role.secret_id_ttl.as_secs())
            {
                return Err(RvError::ErrResponse("ttl cannot be longer than the role's secret_id_ttl".to_string()));
            }
        } else {
            ttl = role.secret_id_ttl;
        }

        let mut secret_id_storage = SecretIdStorageEntry {
            secret_id_num_uses: num_uses,
            secret_id_ttl: ttl,
            cidr_list,
            token_cidr_list: token_bound_cidrs,
            ..Default::default()
        };

        if let Ok(metadata_value) = req.get_data("metadata") {
            secret_id_storage.metadata = metadata_value.as_map().ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        let storage = Arc::as_ref(req.storage.as_ref().unwrap());
        self.register_secret_id_entry(
            storage,
            &role.name,
            secret_id,
            &role.hmac_key,
            &role.secret_id_prefix,
            &mut secret_id_storage,
        )
        .await?;

        let resp_data = json!({
            "secret_id": secret_id,
            "secret_id_accessor": secret_id_storage.secret_id_accessor,
            "secret_id_ttl": self.derive_secret_id_ttl(secret_id_storage.secret_id_ttl).as_secs(),
            "secret_id_num_uses": secret_id_storage.secret_id_num_uses,
        });

        Ok(Some(Response::data_response(resp_data.as_object().cloned())))
    }

    pub async fn write_role_secret_id(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let secret_id = utils::generate_uuid();
        self.update_role_secret_id_common(req, &secret_id).await
    }

    pub async fn write_role_secret_id_lookup(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;
        let secret_id = req.get_data_as_str("secret_id")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.read().await;

        let role = self.get_role(req, &role_name).await?;
        if role.is_none() {
            return Err(RvError::ErrResponse(format!("role {role_name} does not exist")));
        }

        let role = role.unwrap();

        let role_name_hmac = create_hmac(&role.hmac_key, &role.name)?;
        let secret_id_hmac = create_hmac(&role.hmac_key, &secret_id)?;

        let entry_index = format!("{}{}/{}", role.secret_id_prefix, role_name_hmac, secret_id_hmac);

        let lock_entry = self.secret_id_locks.get_lock(&secret_id_hmac);
        let _locked = lock_entry.lock.write().await;

        let storage = Arc::as_ref(req.storage.as_ref().unwrap());

        if let Some(secret_id_entry) =
            self.get_secret_id_storage_entry(storage, &role.secret_id_prefix, &role_name_hmac, &secret_id_hmac).await?
        {
            // If a secret ID entry does not have a corresponding accessor
            // entry, revoke the secret ID immediately
            let accessor_entry = self
                .get_secret_id_accessor_entry(storage, &secret_id_entry.secret_id_accessor, &role.secret_id_prefix)
                .await?;
            if accessor_entry.is_none() {
                req.storage_delete(&entry_index).await?;
                return Err(RvError::ErrResponse("invalid secret_id".to_string()));
            }

            let data = serde_json::to_value(&secret_id_entry)?;
            return Ok(Some(Response::data_response(Some(data.as_object().unwrap().clone()))));
        }

        Ok(None)
    }

    pub async fn write_role_secret_id_destory(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_secret_id_destory(backend, req).await
    }

    pub async fn delete_role_secret_id_destory(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;
        let secret_id = req.get_data_as_str("secret_id")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.read().await;

        let role = self.get_role(req, &role_name).await?;
        if role.is_none() {
            return Err(RvError::ErrResponse(format!("role {role_name} does not exist")));
        }

        let role = role.unwrap();

        let role_name_hmac = create_hmac(&role.hmac_key, &role.name)?;
        let secret_id_hmac = create_hmac(&role.hmac_key, &secret_id)?;

        let entry_index = format!("{}{}/{}", role.secret_id_prefix, role_name_hmac, secret_id_hmac);

        let lock_entry = self.secret_id_locks.get_lock(&secret_id_hmac);
        let _locked = lock_entry.lock.write().await;

        let storage = Arc::as_ref(req.storage.as_ref().unwrap());

        if let Some(secret_id_entry) =
            self.get_secret_id_storage_entry(storage, &role.secret_id_prefix, &role_name_hmac, &secret_id_hmac).await?
        {
            // Delete the accessor of the secret_id first
            self.delete_secret_id_accessor_entry(storage, &secret_id_entry.secret_id_accessor, &role.secret_id_prefix)
                .await?;

            // Delete the storage entry that corresponds to the secret_id
            storage.delete(&entry_index).await?;
        }

        Ok(None)
    }

    pub async fn write_role_secret_id_accessor_lookup(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;
        let secret_id_accessor = req.get_data_as_str("secret_id_accessor")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.read().await;

        let role = self.get_role(req, &role_name).await?;
        if role.is_none() {
            return Err(RvError::ErrResponse(format!("role {role_name} does not exist")));
        }

        let role = role.unwrap();

        let storage = Arc::as_ref(req.storage.as_ref().unwrap());

        if let Some(accessor_entry) =
            self.get_secret_id_accessor_entry(storage, &secret_id_accessor, &role.secret_id_prefix).await?
        {
            let role_name_hmac = create_hmac(&role.hmac_key, &role.name)?;

            let lock_entry = self.secret_id_locks.get_lock(&accessor_entry.secret_id_hmac);
            let _locked = lock_entry.lock.read().await;

            if let Some(secret_id_entry) = self
                .get_secret_id_storage_entry(
                    storage,
                    &role.secret_id_prefix,
                    &role_name_hmac,
                    &accessor_entry.secret_id_hmac,
                )
                .await?
            {
                let data = serde_json::to_value(secret_id_entry)?;
                return Ok(Some(Response::data_response(Some(data.as_object().unwrap().clone()))));
            }
        } else {
            return Err(RvError::ErrResponseStatus(
                404,
                format!("failed to find accessor entry for secret_id_accessor: {secret_id_accessor}"),
            ));
        }

        Ok(None)
    }

    pub async fn write_role_secret_id_accessor_destory(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_role_secret_id_accessor_destory(backend, req).await
    }

    pub async fn delete_role_secret_id_accessor_destory(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let role_name = req.get_data_as_str("role_name")?;
        let secret_id_accessor = req.get_data_as_str("secret_id_accessor")?;

        let lock_entry = self.role_locks.get_lock(&role_name);
        let _locked = lock_entry.lock.write().await;

        // secret_id is indexed based on HMACed role_name and HMACed secret_id.
        // Get the role details to fetch the role_id and accessor to get
        // the HMACed secret_id.

        let role = self.get_role(req, &role_name).await?;
        if role.is_none() {
            return Err(RvError::ErrResponse(format!("role {role_name} does not exist")));
        }

        let role = role.unwrap();

        let storage = Arc::as_ref(req.storage.as_ref().unwrap());

        if let Some(accessor_entry) =
            self.get_secret_id_accessor_entry(storage, &secret_id_accessor, &role.secret_id_prefix).await?
        {
            let role_name_hmac = create_hmac(&role.hmac_key, &role.name)?;

            let lock_entry = self.secret_id_locks.get_lock(&accessor_entry.secret_id_hmac);
            let _locked = lock_entry.lock.write().await;

            // Verify we have a valid secret_id storage entry
            if self
                .get_secret_id_storage_entry(
                    storage,
                    &role.secret_id_prefix,
                    &role_name_hmac,
                    &accessor_entry.secret_id_hmac,
                )
                .await?
                .is_none()
            {
                return Err(RvError::ErrResponseStatus(
                    403,
                    format!("invalid secret_id_accessor: {secret_id_accessor}"),
                ));
            }

            let entry_index = format!("{}{}/{}", role.secret_id_prefix, role_name_hmac, &accessor_entry.secret_id_hmac);

            let storage = Arc::as_ref(req.storage.as_ref().unwrap());

            // Delete the accessor of the secret_id first
            self.delete_secret_id_accessor_entry(storage, &secret_id_accessor, &role.secret_id_prefix).await?;

            storage.delete(&entry_index).await?;
        } else {
            return Err(RvError::ErrResponseStatus(
                404,
                format!("failed to find accessor entry for secret_id_accessor: {secret_id_accessor}"),
            ));
        }

        Ok(None)
    }

    pub async fn write_role_custom_secret_id(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.update_role_secret_id_common(req, req.get_data("secret_id")?.as_str().unwrap_or("")).await
    }
}

#[cfg(test)]
mod test {
    use std::{default::Default, sync::Arc};

    use serde_json::{json, Value};

    use super::{
        super::{
            test::{generate_secret_id, test_delete_role, test_login, test_write_role},
            AppRoleModule, SECRET_ID_PREFIX,
        },
        *,
    };
    use crate::{
        logical::{Operation, Request},
        modules::auth::expiration::MAX_LEASE_DURATION_SECS,
        storage::Storage,
        test_utils::{
            new_unseal_test_rusty_vault, test_delete_api, test_list_api, test_mount_auth_api, test_read_api,
            test_write_api,
        },
    };

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_read_local_secret_ids() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_read_local_secret_ids").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        // Create a role
        let data = json!({
            "local_secret_ids": true,
            "bind_secret_id":   true,
        })
        .as_object()
        .unwrap()
        .clone();

        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole", true, Some(data.clone())).await;
        assert!(resp.is_ok());

        // Get the role field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/testrole/local-secret-ids", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["local_secret_ids"].as_bool().unwrap(), data["local_secret_ids"].as_bool().unwrap());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_local_non_secret_ids() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_local_non_secret_ids").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        // Create a role with local_secret_ids set
        let data = json!({
            "policies": ["default", "role1policy"],
            "local_secret_ids": true,
            "bind_secret_id":   true,
        })
        .as_object()
        .cloned();
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole1", true, data).await;
        assert!(resp.is_ok());

        // Create another role without setting local_secret_ids
        let data = json!({
            "policies": ["default", "role1policy"],
            "bind_secret_id":   true,
        })
        .as_object()
        .cloned();
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole2", true, data).await;
        assert!(resp.is_ok());

        // Create secret IDs on testrole1
        let len = 10;
        for _i in 0..len {
            let ret = test_write_api(&core, &root_token, "auth/approle/role/testrole1/secret-id", true, None).await;
            assert!(ret.is_ok());
        }

        // Check the number of secret IDs generated
        let resp = test_list_api(&core, &root_token, "auth/approle/role/testrole1/secret-id", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert!(resp_data["keys"].is_array());
        assert_eq!(resp_data["keys"].as_array().unwrap().len(), len);

        // Create secret IDs on testrole2
        for _i in 0..len {
            let ret = test_write_api(&core, &root_token, "auth/approle/role/testrole2/secret-id", true, None).await;
            assert!(ret.is_ok());
        }

        // Check the number of secret IDs generated
        let resp = test_list_api(&core, &root_token, "auth/approle/role/testrole2/secret-id", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert!(resp_data["keys"].is_array());
        assert_eq!(resp_data["keys"].as_array().unwrap().len(), len);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_upgrade_secret_id_prefix() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_upgrade_secret_id_prefix").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let approle_module = core.module_manager.get_module::<AppRoleModule>("approle").unwrap();

        let mut req = Request::new("/auth/approle/testrole");
        req.operation = Operation::Write;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);

        let role_entry = RoleEntry {
            role_id: "testroleid".to_string(),
            hmac_key: "testhmackey".to_string(),
            bind_secret_id: true,
            bound_cidr_list_old: "127.0.0.1/18,192.178.1.2/24".to_string(),
            ..Default::default()
        };
        let resp = approle_module.set_role(&mut req, "testrole", &role_entry, "").await;
        assert!(resp.is_ok());

        // Reading the role entry should upgrade it to contain secret_id_prefix
        let resp = approle_module.get_role(&mut req, "testrole").await;
        assert!(resp.is_ok());
        let role_entry = resp.unwrap().unwrap();
        assert_ne!(role_entry.secret_id_prefix, "");

        // Ensure that the API response contains local_secret_ids
        req.operation = Operation::Read;
        req.path = "auth/approle/role/testrole".to_string();
        req.client_token = root_token.to_string();
        let _resp = core.handle_request(&mut req).await;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);

        let mock_backend = approle_module.new_backend();
        let resp = approle_module.read_role(&mock_backend, &mut req).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert!(!resp_data["local_secret_ids"].as_bool().unwrap());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_local_secret_id_immutablility() {
        let (_rvault, core, root_token) =
            new_unseal_test_rusty_vault("test_approle_local_secret_id_immutablility").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        // Create a role with local_secret_ids set
        let data = json!({
            "policies": ["default"],
            "bind_secret_id":   true,
            "local_secret_ids": true,
            "bound_cidr_list": ["127.0.0.1/18", "192.178.1.2/24"],
        })
        .as_object()
        .cloned();
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole", true, data.clone()).await;
        assert!(resp.is_ok());

        // Attempt to modify local_secret_ids should fail
        let _ = test_write_api(&core, &root_token, "auth/approle/role/testrole", false, data.clone()).await;
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_upgrade_bound_cidr_list() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_upgrade_bound_cidr_list").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        // Create a role with bound_cidr_list set
        let data = json!({
            "policies": ["default"],
            "bind_secret_id":   true,
            "bound_cidr_list": ["127.0.0.1/18", "192.178.1.2/24"],
        })
        .as_object()
        .unwrap()
        .clone();
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole", true, Some(data.clone())).await;
        assert!(resp.is_ok());

        // Read the role and check that the bound_cidr_list is set properly
        let resp = test_read_api(&core, &root_token, "auth/approle/role/testrole", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let expected: Vec<Value> =
            data["bound_cidr_list"].as_comma_string_slice().unwrap().iter().map(|s| Value::String(s.clone())).collect();
        assert_eq!(resp_data["secret_id_bound_cidrs"].as_array().unwrap().clone(), expected);

        let approle_module = core.module_manager.get_module::<AppRoleModule>("approle").unwrap();

        let mut req = Request::new("/auth/approle/testrole");
        req.operation = Operation::Write;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);

        // Modify the storage entry of the role to hold the old style string typed bound_cidr_list
        let role_entry = RoleEntry {
            role_id: "testroleid".to_string(),
            hmac_key: "testhmackey".to_string(),
            bind_secret_id: true,
            bound_cidr_list_old: "127.0.0.1/18,192.178.1.2/24".to_string(),
            secret_id_prefix: SECRET_ID_PREFIX.to_string(),
            ..Default::default()
        };
        let resp = approle_module.set_role(&mut req, "testrole", &role_entry, "").await;
        assert!(resp.is_ok());
        let expected: Vec<String> = role_entry.bound_cidr_list_old.split(',').map(|s| s.to_string()).collect();

        // Read the role. The upgrade code should have migrated the old type to the new type
        let resp = approle_module.get_role(&mut req, "testrole").await;
        assert!(resp.is_ok());
        let role_entry = resp.unwrap().unwrap();
        assert_eq!(role_entry.secret_id_bound_cidrs, expected);
        assert_eq!(role_entry.bound_cidr_list_old.len(), 0);
        assert_eq!(role_entry.bound_cidr_list.len(), 0);

        // Create a secret-id by supplying a subset of the role's CIDR blocks with the new type
        let data = json!({
            "cidr_list": ["127.0.0.1/24"],
        })
        .as_object()
        .cloned();
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole/secret-id", true, data).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let secret_id = resp_data["secret_id"].as_str().unwrap();
        assert_ne!(secret_id, "");
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_name_lower_casing() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_role_name_lower_casing").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let approle_module = core.module_manager.get_module::<AppRoleModule>("approle").unwrap();

        let mut req = Request::new("/auth/approle/testrole");
        req.operation = Operation::Write;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);

        // Create a role with lower_case_role_name is false
        let role_entry = RoleEntry {
            role_id: "testroleid".to_string(),
            hmac_key: "testhmackey".to_string(),
            bind_secret_id: true,
            lower_case_role_name: false,
            secret_id_prefix: SECRET_ID_PREFIX.to_string(),
            ..Default::default()
        };
        let resp = approle_module.set_role(&mut req, "testRoleName", &role_entry, "").await;
        assert!(resp.is_ok());

        req.operation = Operation::Write;
        req.path = "auth/approle/role/testRoleName/secret-id".to_string();
        req.client_token = root_token.to_string();
        let _resp = core.handle_request(&mut req).await;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);

        let mock_backend = approle_module.new_backend();
        let resp = approle_module.write_role_secret_id(&mock_backend, &mut req).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let secret_id = resp_data["secret_id"].as_str().unwrap();
        let role_id = "testroleid";

        // Regular login flow. This should succeed
        let data = json!({
            "role_id": role_id,
            "secret_id": secret_id,
        })
        .as_object()
        .cloned();
        req.path = "auth/approle/login".to_string();
        req.operation = Operation::Write;
        req.body = data;
        let _resp = core.handle_request(&mut req).await;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);
        let resp = approle_module.login(&mock_backend, &mut req).await;
        assert!(resp.is_ok());

        // Lower case the role name when generating the secret id
        req.path = "auth/approle/role/testrolename/secret-id".to_string();
        req.operation = Operation::Write;
        req.body = None;
        let _resp = core.handle_request(&mut req).await;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);
        let resp = approle_module.write_role_secret_id(&mock_backend, &mut req).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let secret_id = resp_data["secret_id"].as_str().unwrap();

        // Login should fail
        let data = json!({
            "role_id": role_id,
            "secret_id": secret_id,
        })
        .as_object()
        .cloned();
        req.path = "auth/approle/login".to_string();
        req.operation = Operation::Write;
        req.body = data;
        let _resp = core.handle_request(&mut req).await;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);
        let resp = approle_module.login(&mock_backend, &mut req).await;
        assert!(resp.is_err());

        // Delete the role and create it again. This time don't directly persist
        // it, but route the request to the creation handler so that it sets the
        // lower_case_role_name to true.
        req.path = "auth/approle/role/testRoleName".to_string();
        req.operation = Operation::Delete;
        req.body = None;
        let _resp = core.handle_request(&mut req).await;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);
        let resp = approle_module.delete_role(&mock_backend, &mut req).await;
        assert!(resp.is_ok());

        let data = json!({
            "policies": ["default"],
            "bind_secret_id":   true,
        })
        .as_object()
        .cloned();
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testRoleName", true, data).await;
        assert!(resp.is_ok());

        // Create secret id with lower cased role name
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrolename/secret-id", true, None).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let secret_id = resp_data["secret_id"].as_str().unwrap();

        let resp = test_read_api(&core, &root_token, "auth/approle/role/testrolename/role-id", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let role_id = resp_data["role_id"].as_str().unwrap();

        // Login should pass
        let _ = test_login(&core, "approle", role_id, secret_id, true).await;

        // Lookup of secret ID should work in case-insensitive manner
        let data = json!({
            "secret_id": secret_id,
        })
        .as_object()
        .cloned();
        let resp =
            test_write_api(&core, &root_token, "auth/approle/role/testrolename/secret-id/lookup", true, data).await;
        assert!(resp.is_ok());

        // Listing of secret IDs should work in case-insensitive manner
        let resp = test_list_api(&core, &root_token, "auth/approle/role/testrolename/secret-id", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let keys = resp_data["keys"].as_array().unwrap();
        assert_eq!(keys.len(), 1);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_read_set_index() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_role_read_set_index").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let approle_module = core.module_manager.get_module::<AppRoleModule>("approle").unwrap();
        let mock_backend = approle_module.new_backend();

        // Create a role
        let mut req = Request::new("/auth/approle/testrole");
        req.operation = Operation::Write;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);
        let role_entry = RoleEntry {
            role_id: "testroleid".to_string(),
            hmac_key: "testhmackey".to_string(),
            bind_secret_id: true,
            secret_id_prefix: SECRET_ID_PREFIX.to_string(),
            ..Default::default()
        };
        let resp = approle_module.set_role(&mut req, "testrole", &role_entry, "").await;
        assert!(resp.is_ok());

        // Get the role ID
        req.operation = Operation::Read;
        req.path = "auth/approle/role/testrole/role-id".to_string();
        req.client_token = root_token.to_string();
        let _resp = core.handle_request(&mut req).await;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);
        let resp = approle_module.read_role_role_id(&mock_backend, &mut req).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let role_id = resp_data["role_id"].as_str().unwrap();

        // Delete the role ID index
        req.operation = Operation::Write;
        req.path = "auth/approle/role/testrole/role-id".to_string();
        req.client_token = root_token.to_string();
        let _resp = core.handle_request(&mut req).await;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);
        let resp = approle_module.delete_role_id(&mut req, role_id).await;
        assert!(resp.is_ok());

        // Read the role again. This should add the index and return a warning
        req.operation = Operation::Read;
        req.path = "auth/approle/role/testrole".to_string();
        req.client_token = root_token.to_string();
        let _resp = core.handle_request(&mut req).await;
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);
        let resp = approle_module.read_role(&mock_backend, &mut req).await;
        assert!(resp.is_ok());
        let resp = resp.unwrap().unwrap();
        assert!(resp.warnings.contains(&"Role identifier was missing an index back to role name".to_string()));

        // Check if the index has been successfully created
        req.storage = core.get_system_view().map(|arc| arc as Arc<dyn Storage>);
        let role_id_entry = approle_module.get_role_id(&mut req, role_id).await;
        assert!(role_id_entry.is_ok());
        let role_id_entry = role_id_entry.unwrap().unwrap();
        assert_eq!(role_id_entry.name, "testrole");

        // Check if updating and reading of roles work and that there are no lock
        // contentions dangling due to previous operation
        let data = json!({
            "policies": ["default"],
            "bind_secret_id":   true,
        })
        .as_object()
        .cloned();
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole", true, data).await;
        assert!(resp.is_ok());
        let resp = test_read_api(&core, &root_token, "auth/approle/role/testrole", true).await;
        assert!(resp.is_ok());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_cidr_subset() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_cidr_subset").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let mut role_data = json!({
            "role_id": "role-id-123",
            "policies": "a,b",
            "bound_cidr_list": "127.0.0.1/24",
        })
        .as_object()
        .unwrap()
        .clone();
        let resp =
            test_write_api(&core, &root_token, "auth/approle/role/testrole1", true, Some(role_data.clone())).await;
        assert!(resp.is_ok());

        let resp = test_read_api(&core, &root_token, "auth/approle/role/testrole", true).await;
        assert!(resp.is_ok());

        let mut secret_data = json!({
            "cidr_list": ["127.0.0.1/16"],
        })
        .as_object()
        .unwrap()
        .clone();
        let resp = test_write_api(
            &core,
            &root_token,
            "auth/approle/role/testrole1/secret-id",
            false,
            Some(secret_data.clone()),
        )
        .await;
        assert!(resp.is_err());

        role_data["bound_cidr_list"] = Value::from("192.168.27.29/16,172.245.30.40/24,10.20.30.40/30");
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole1", true, Some(role_data)).await;
        assert!(resp.is_ok());

        secret_data["cidr_list"] = Value::from("192.168.27.29/20,172.245.30.40/25,10.20.30.40/32");
        let resp =
            test_write_api(&core, &root_token, "auth/approle/role/testrole1/secret-id", true, Some(secret_data)).await;
        assert!(resp.is_ok());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_token_bound_cidr_subset_32_mask() {
        let (_rvault, core, root_token) =
            new_unseal_test_rusty_vault("test_approle_token_bound_cidr_subset_32_mask").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let role_data = json!({
            "role_id": "role-id-123",
            "policies": "a,b",
            "token_bound_cidrs": "127.0.0.1/32",
        })
        .as_object()
        .cloned();
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole1", true, role_data).await;
        assert!(resp.is_ok());

        let resp = test_read_api(&core, &root_token, "auth/approle/role/testrole", true).await;
        assert!(resp.is_ok());

        let mut secret_data = json!({
            "token_bound_cidrs": ["127.0.0.1/32"],
        })
        .as_object()
        .unwrap()
        .clone();
        let resp = test_write_api(
            &core,
            &root_token,
            "auth/approle/role/testrole1/secret-id",
            true,
            Some(secret_data.clone()),
        )
        .await;
        assert!(resp.is_ok());

        secret_data["token_bound_cidrs"] = Value::from("127.0.0.1/24");
        let resp =
            test_write_api(&core, &root_token, "auth/approle/role/testrole1/secret-id", false, Some(secret_data)).await;
        assert!(resp.is_err());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_constraints() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_role_constraints").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        // Set bind_secret_id, which is enabled by default
        let mut role_data = json!({
            "role_id": "role-id-123",
            "policies": "a,b",
        })
        .as_object()
        .unwrap()
        .clone();
        let resp =
            test_write_api(&core, &root_token, "auth/approle/role/testrole1", true, Some(role_data.clone())).await;
        assert!(resp.is_ok());

        // Set bound_cidr_list alone by explicitly disabling bind_secret_id
        role_data.insert("bind_secret_id".to_string(), Value::from(false));
        role_data.insert("token_bound_cidrs".to_string(), Value::from("0.0.0.0/0"));
        let resp =
            test_write_api(&core, &root_token, "auth/approle/role/testrole1", true, Some(role_data.clone())).await;
        assert!(resp.is_ok());

        // Remove both constraints
        role_data["bind_secret_id"] = Value::from(false);
        role_data["token_bound_cidrs"] = Value::from("");
        let resp =
            test_write_api(&core, &root_token, "auth/approle/role/testrole1", false, Some(role_data.clone())).await;
        assert!(resp.is_err());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_update_role_id() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_update_role_id").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        test_write_role(&core, &root_token, "approle", "testrole1", "role-id-123", "a,b", true).await;

        let role_id_data = json!({
            "role_id": "customroleid",
        })
        .as_object()
        .cloned();
        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole1/role-id", true, role_id_data).await;
        assert!(resp.is_ok());

        let resp = test_write_api(&core, &root_token, "auth/approle/role/testrole1/secret-id", true, None).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let secret_id = resp_data["secret_id"].as_str().unwrap();

        // Login should fail
        let _ = test_login(&core, "approle", "role-id-123", secret_id, false).await;

        // Login should pass
        let _ = test_login(&core, "approle", "customroleid", secret_id, true).await;
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_id_uniqueness() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_role_id_uniqueness").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        test_write_role(&core, &root_token, "approle", "testrole1", "role-id-123", "a,b", true).await;

        test_write_role(&core, &root_token, "approle", "testrole2", "role-id-123", "a,b", false).await;

        test_write_role(&core, &root_token, "approle", "testrole2", "role-id-456", "a,b", true).await;

        test_write_role(&core, &root_token, "approle", "testrole2", "role-id-123", "a,b", false).await;

        test_write_role(&core, &root_token, "approle", "testrole1", "role-id-456", "a,b", false).await;

        let mut role_id_data = json!({
            "role_id": "role-id-456",
        })
        .as_object()
        .unwrap()
        .clone();
        let resp = test_write_api(
            &core,
            &root_token,
            "auth/approle/role/testrole1/role-id",
            false,
            Some(role_id_data.clone()),
        )
        .await;
        assert!(resp.is_err());

        role_id_data["role_id"] = Value::from("role-id-123");
        let resp = test_write_api(
            &core,
            &root_token,
            "auth/approle/role/testrole2/role-id",
            false,
            Some(role_id_data.clone()),
        )
        .await;
        assert!(resp.is_err());

        role_id_data["role_id"] = Value::from("role-id-2000");
        let resp =
            test_write_api(&core, &root_token, "auth/approle/role/testrole2/role-id", true, Some(role_id_data.clone()))
                .await;
        assert!(resp.is_ok());

        role_id_data["role_id"] = Value::from("role-id-1000");
        let resp =
            test_write_api(&core, &root_token, "auth/approle/role/testrole1/role-id", true, Some(role_id_data.clone()))
                .await;
        assert!(resp.is_ok());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_delete_secret_id() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_role_delete_secret_id").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        test_write_role(&core, &root_token, "approle", "role1", "", "a,b", true).await;

        let _ = generate_secret_id(&core, &root_token, "approle", "role1").await;
        let _ = generate_secret_id(&core, &root_token, "approle", "role1").await;
        let _ = generate_secret_id(&core, &root_token, "approle", "role1").await;

        let resp = test_list_api(&core, &root_token, "auth/approle/role/role1/secret-id", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let keys = resp_data["keys"].as_array().unwrap();
        assert_eq!(keys.len(), 3);

        test_delete_role(&core, &root_token, "approle", "role1").await;
        let _ = test_list_api(&core, &root_token, "auth/approle/role/role1/secret-id", false).await;
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_lookup_and_destroy_role_secret_id() {
        let (_rvault, core, root_token) =
            new_unseal_test_rusty_vault("test_approle_lookup_and_destroy_role_secret_id").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        test_write_role(&core, &root_token, "approle", "role1", "", "a,b", true).await;

        let (secret_id, _) = generate_secret_id(&core, &root_token, "approle", "role1").await;

        let secret_id_data = json!({
            "secret_id": secret_id,
        })
        .as_object()
        .cloned();
        let resp = test_write_api(
            &core,
            &root_token,
            "auth/approle/role/role1/secret-id/lookup",
            true,
            secret_id_data.clone(),
        )
        .await;
        assert!(resp.unwrap().unwrap().data.is_some());

        let _ = test_delete_api(
            &core,
            &root_token,
            "auth/approle/role/role1/secret-id/destroy",
            true,
            secret_id_data.clone(),
        )
        .await;
        let resp =
            test_write_api(&core, &root_token, "auth/approle/role/role1/secret-id/lookup", true, secret_id_data).await;
        assert!(resp.unwrap().is_none());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_lookup_and_destroy_role_secret_id_accessor() {
        let (_rvault, core, root_token) =
            new_unseal_test_rusty_vault("test_approle_lookup_and_destroy_role_secret_id_accessor").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        test_write_role(&core, &root_token, "approle", "role1", "", "a,b", true).await;

        let _ = generate_secret_id(&core, &root_token, "approle", "role1").await;

        let resp = test_list_api(&core, &root_token, "auth/approle/role/role1/secret-id", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let keys = resp_data["keys"].as_array().unwrap();
        assert_eq!(keys.len(), 1);

        let hmac_secret_id = keys[0].as_str().unwrap();
        let hmac_data = json!({
            "secret_id_accessor": hmac_secret_id,
        })
        .as_object()
        .cloned();
        let resp = test_write_api(
            &core,
            &root_token,
            "auth/approle/role/role1/secret-id-accessor/lookup",
            true,
            hmac_data.clone(),
        )
        .await;
        assert!(resp.unwrap().unwrap().data.is_some());

        let _ = test_delete_api(
            &core,
            &root_token,
            "auth/approle/role/role1/secret-id-accessor/destroy",
            true,
            hmac_data.clone(),
        )
        .await;
        let _ =
            test_write_api(&core, &root_token, "auth/approle/role/role1/secret-id-accessor/lookup", false, hmac_data)
                .await;
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_lookup_role_secret_id_accessor() {
        let (_rvault, core, root_token) =
            new_unseal_test_rusty_vault("test_approle_lookup_role_secret_id_accessor").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        test_write_role(&core, &root_token, "approle", "role1", "", "a,b", true).await;

        let hmac_data = json!({
            "secret_id_accessor": "invalid",
        })
        .as_object()
        .cloned();
        let _resp =
            test_write_api(&core, &root_token, "auth/approle/role/role1/secret-id-accessor/lookup", false, hmac_data)
                .await;
        // TODO: resp should ok
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_list_role_secret_id() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_list_role_secret_id").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        test_write_role(&core, &root_token, "approle", "role1", "", "a,b", true).await;

        // Create 5 'secret_id's
        let _ = generate_secret_id(&core, &root_token, "approle", "role1").await;
        let _ = generate_secret_id(&core, &root_token, "approle", "role1").await;
        let _ = generate_secret_id(&core, &root_token, "approle", "role1").await;
        let _ = generate_secret_id(&core, &root_token, "approle", "role1").await;
        let _ = generate_secret_id(&core, &root_token, "approle", "role1").await;

        let resp = test_list_api(&core, &root_token, "auth/approle/role/role1/secret-id/", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let keys = resp_data["keys"].as_array().unwrap();
        assert_eq!(keys.len(), 5);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_list_role() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_list_role").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        test_write_role(&core, &root_token, "approle", "role1", "", "a,b", true).await;
        test_write_role(&core, &root_token, "approle", "role2", "", "c,d", true).await;
        test_write_role(&core, &root_token, "approle", "role3", "", "e,f", true).await;
        test_write_role(&core, &root_token, "approle", "role4", "", "g,h", true).await;
        test_write_role(&core, &root_token, "approle", "role5", "", "i,j", true).await;

        let resp = test_list_api(&core, &root_token, "auth/approle/role", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let mut keys = resp_data["keys"].as_array().unwrap().clone();
        keys.sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
        assert_eq!(keys.len(), 5);
        let expect = json!(["role1", "role2", "role3", "role4", "role5"]);
        assert_eq!(expect.as_array().unwrap().clone(), keys);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_secret_id_without_fields() {
        let (_rvault, core, root_token) =
            new_unseal_test_rusty_vault("test_approle_role_secret_id_without_fields").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let role_data = json!({
            "policies": "p,q,r,s",
            "secret_id_num_uses": 10,
            "secret_id_ttl":      300,
            "token_ttl":          400,
            "token_max_ttl":      500,
        })
        .as_object()
        .unwrap()
        .clone();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, Some(role_data.clone())).await;

        let resp = test_write_api(&core, &root_token, "auth/approle/role/role1/secret-id", true, None).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let secret_id = resp_data["secret_id"].as_str().unwrap();
        let secret_id_ttl = resp_data["secret_id_ttl"].as_int().unwrap();
        let secret_id_num_uses = resp_data["secret_id_num_uses"].as_int().unwrap();
        assert_ne!(secret_id, "");
        assert_eq!(secret_id_ttl, role_data["secret_id_ttl"].as_int().unwrap());
        assert_eq!(secret_id_num_uses, role_data["secret_id_num_uses"].as_int().unwrap());

        let secret_id_data = json!({
            "secret_id": "abcd123",
        })
        .as_object()
        .unwrap()
        .clone();
        let resp = test_write_api(
            &core,
            &root_token,
            "auth/approle/role/role1/custom-secret-id",
            true,
            Some(secret_id_data.clone()),
        )
        .await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let secret_id = resp_data["secret_id"].as_str().unwrap();
        let secret_id_ttl = resp_data["secret_id_ttl"].as_int().unwrap();
        let secret_id_num_uses = resp_data["secret_id_num_uses"].as_int().unwrap();
        assert_eq!(secret_id, secret_id_data["secret_id"].as_str().unwrap());
        assert_eq!(secret_id_ttl, role_data["secret_id_ttl"].as_int().unwrap());
        assert_eq!(secret_id_num_uses, role_data["secret_id_num_uses"].as_int().unwrap());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_secret_id_with_valid_fields() {
        let (_rvault, core, root_token) =
            new_unseal_test_rusty_vault("test_approle_role_secret_id_with_valid_fields").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let role_data = json!({
            "policies": "p,q,r,s",
            "secret_id_num_uses": 0,
            "secret_id_ttl":      0,
            "token_ttl":          400,
            "token_max_ttl":      500,
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, role_data).await;

        let cases = vec![
            json!({"name": "finite num_uses and ttl", "payload": {"secret_id": "finite", "ttl": 5, "num_uses": 5}}),
            json!({"name": "infinite num_uses and ttl", "payload": {"secret_id": "infinite", "ttl": 0, "num_uses": 0}}),
            json!({"name": "finite num_uses and infinite ttl", "payload": {"secret_id": "maxed1", "ttl": 0, "num_uses": 5}}),
            json!({"name": "infinite num_uses and finite ttl", "payload": {"secret_id": "maxed2", "ttl": 5, "num_uses": 0}}),
        ];

        for case in cases.iter() {
            let secret_id_data = case["payload"].as_object().unwrap().clone();
            let resp = test_write_api(
                &core,
                &root_token,
                "auth/approle/role/role1/secret-id",
                true,
                Some(secret_id_data.clone()),
            )
            .await;
            assert!(resp.is_ok());
            let resp_data = resp.unwrap().unwrap().data.unwrap();
            let secret_id = resp_data["secret_id"].as_str().unwrap();
            let secret_id_ttl = resp_data["secret_id_ttl"].as_int().unwrap();
            let secret_id_num_uses = resp_data["secret_id_num_uses"].as_int().unwrap();
            assert_ne!(secret_id, "");
            assert_eq!(secret_id_ttl, secret_id_data["ttl"].as_int().unwrap());
            assert_eq!(secret_id_num_uses, secret_id_data["num_uses"].as_int().unwrap());

            let resp = test_write_api(
                &core,
                &root_token,
                "auth/approle/role/role1/custom-secret-id",
                true,
                Some(secret_id_data.clone()),
            )
            .await;
            assert!(resp.is_ok());
            let resp_data = resp.unwrap().unwrap().data.unwrap();
            let secret_id = resp_data["secret_id"].as_str().unwrap();
            let secret_id_ttl = resp_data["secret_id_ttl"].as_int().unwrap();
            let secret_id_num_uses = resp_data["secret_id_num_uses"].as_int().unwrap();
            assert_eq!(secret_id, secret_id_data["secret_id"].as_str().unwrap());
            assert_eq!(secret_id_ttl, secret_id_data["ttl"].as_int().unwrap());
            assert_eq!(secret_id_num_uses, secret_id_data["num_uses"].as_int().unwrap());
        }
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_secret_id_with_invalid_fields() {
        let (_rvault, core, root_token) =
            new_unseal_test_rusty_vault("test_approle_role_secret_id_with_invalid_fields").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let cases = vec![
            json!({
                "name": "infinite role secret id ttl",
                "options": {
                    "secret_id_num_uses": 1,
                    "secret_id_ttl": 0,
                },
                "cases": [{
                    "name": "higher num_uses",
                    "payload": {"secret_id": "abcd123", "ttl": 0, "num_uses": 2},
                    "expected": "num_uses cannot be higher than the role's secret_id_num_uses",
                }],
            }),
            json!({
                "name": "infinite role num_uses",
                "options": {
                    "secret_id_num_uses": 0,
                    "secret_id_ttl": 1,
                },
                "cases": [{
                    "name": "longer ttl",
                    "payload": {"secret_id": "abcd123", "ttl": 2, "num_uses": 0},
                    "expected": "ttl cannot be longer than the role's secret_id_ttl",
                }],
            }),
            json!({
                "name": "finite role ttl and num_uses",
                "options": {
                    "secret_id_num_uses": 2,
                    "secret_id_ttl": 2,
                },
                "cases": [{
                    "name": "infinite ttl",
                    "payload": {"secret_id": "abcd123", "ttl": 0, "num_uses": 1},
                    "expected": "ttl cannot be longer than the role's secret_id_ttl",
                },
                {
                    "name": "infinite num_uses",
                    "payload": {"secret_id": "abcd123", "ttl": 1, "num_uses": 0},
                    "expected": "num_uses cannot be higher than the role's secret_id_num_uses",
                }],
            }),
            json!({
                "name": "mixed role ttl and num_uses",
                "options": {
                    "secret_id_num_uses": 400,
                    "secret_id_ttl": 500,
                },
                "cases": [{
                    "name": "negative num_uses",
                    "payload": {"secret_id": "abcd123", "ttl": 0, "num_uses": -1},
                    "expected": "num_uses cannot be negative",
                }],
            }),
        ];

        for (i, case) in cases.iter().enumerate() {
            let mut role_data = json!({
                "policies": "p,q,r,s",
                "secret_id_num_uses": 0,
                "secret_id_ttl":      0,
                "token_ttl":          400,
                "token_max_ttl":      500,
            })
            .as_object()
            .unwrap()
            .clone();
            role_data["secret_id_num_uses"] = case["options"]["secret_id_num_uses"].clone();
            role_data["secret_id_ttl"] = case["options"]["secret_id_ttl"].clone();
            let _ = test_write_api(
                &core,
                &root_token,
                format!("auth/approle/role/role{}", i).as_str(),
                true,
                Some(role_data.clone()),
            )
            .await;

            for tc in case["cases"].as_array().unwrap().iter() {
                let secret_id_data = tc["payload"].as_object().unwrap().clone();
                let resp = test_write_api(
                    &core,
                    &root_token,
                    format!("auth/approle/role/role{}/secret-id", i).as_str(),
                    false,
                    Some(secret_id_data.clone()),
                )
                .await;
                if let Err(RvError::ErrResponse(err_text)) = resp {
                    assert_eq!(err_text, tc["expected"].as_str().unwrap());
                }
                let resp = test_write_api(
                    &core,
                    &root_token,
                    format!("auth/approle/role/role{}/custom-secret-id", i).as_str(),
                    false,
                    Some(secret_id_data.clone()),
                )
                .await;
                if let Err(RvError::ErrResponse(err_text)) = resp {
                    assert_eq!(err_text, tc["expected"].as_str().unwrap());
                }
            }
        }
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_crud() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_role_crud").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let req_data = json!({
            "policies": "p,q,r,s",
            "secret_id_num_uses": 10,
            "secret_id_ttl":      300,
            "token_ttl":          400,
            "token_max_ttl":      500,
            "token_num_uses":     600,
            "secret_id_bound_cidrs": "127.0.0.1/32,127.0.0.1/16",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, req_data).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();

        let expected = json!({
            "bind_secret_id":        true,
            "local_secret_ids":      false,
            "policies":              ["p", "q", "r", "s"],
            "secret_id_num_uses":    10,
            "secret_id_ttl":         300,
            "token_ttl":             400,
            "token_max_ttl":         500,
            "token_num_uses":        600,
            "token_no_default_policy": false,
            "secret_id_bound_cidrs": ["127.0.0.1/32", "127.0.0.1/16"],
            "token_period":          0,
            "token_explicit_max_ttl":0,
            "token_bound_cidrs":     [],
            "token_policies":        ["p", "q", "r", "s"],
            "token_type":            "default",
        });
        assert_eq!(expected.as_object().unwrap().clone(), resp_data);

        let req_data = json!({
            "role_id": "test_role_id",
            "policies": "a,b,c,d",
            "secret_id_num_uses": 100,
            "secret_id_ttl":      3000,
            "token_ttl":          4000,
            "token_max_ttl":      5000,
            "period":      "5m",
        })
        .as_object()
        .unwrap()
        .clone();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, Some(req_data.clone())).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();

        let expected = json!({
            "bind_secret_id":        true,
            "local_secret_ids":      false,
            "policies":              ["a", "b", "c", "d"],
            "secret_id_num_uses":    100,
            "secret_id_ttl":         3000,
            "token_ttl":             4000,
            "token_max_ttl":         5000,
            "token_num_uses":        600,
            "token_no_default_policy": false,
            "secret_id_bound_cidrs": ["127.0.0.1/32", "127.0.0.1/16"],
            "period":                300,
            "token_period":          300,
            "token_explicit_max_ttl":0,
            "token_bound_cidrs":     [],
            "token_policies":        ["a", "b", "c", "d"],
            "token_type":            "default",
        });
        assert_eq!(expected.as_object().unwrap().clone(), resp_data);

        // RU for role_id field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/role-id", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let role_id = resp_data["role_id"].as_str().unwrap();
        assert_eq!(role_id, req_data["role_id"].as_str().unwrap());

        let req_data = json!({
            "role_id": "custom_role_id",
        })
        .as_object()
        .unwrap()
        .clone();
        let _ =
            test_write_api(&core, &root_token, "auth/approle/role/role1/role-id", true, Some(req_data.clone())).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/role-id", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["role_id"].as_str().unwrap(), req_data["role_id"].as_str().unwrap());

        // RUD for bind_secret_id field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/bind-secret-id", true).await;
        assert!(resp.is_ok());

        let req_data = json!({
            "bind_secret_id": false,
        })
        .as_object()
        .unwrap()
        .clone();
        let _ =
            test_write_api(&core, &root_token, "auth/approle/role/role1/bind-secret-id", true, Some(req_data.clone()))
                .await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/bind-secret-id", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["bind_secret_id"].as_bool().unwrap(), req_data["bind_secret_id"].as_bool().unwrap());

        let _ = test_delete_api(&core, &root_token, "auth/approle/role/role1/bind-secret-id", true, None).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/bind-secret-id", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["bind_secret_id"].as_bool().unwrap(), true);

        // RUD for policies field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/policies", true).await;
        assert!(resp.is_ok());

        let req_data = json!({
            "policies": "a1,b1,c1,d1",
        })
        .as_object()
        .unwrap()
        .clone();
        let _ =
            test_write_api(&core, &root_token, "auth/approle/role/role1/policies", true, Some(req_data.clone())).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/policies", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(
            resp_data["policies"].as_comma_string_slice().unwrap(),
            req_data["policies"].as_comma_string_slice().unwrap()
        );
        assert_eq!(
            resp_data["token_policies"].as_comma_string_slice().unwrap(),
            req_data["policies"].as_comma_string_slice().unwrap()
        );

        let _ = test_delete_api(&core, &root_token, "auth/approle/role/role1/policies", true, None).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/policies", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_policies"].as_comma_string_slice().unwrap().len(), 0);

        // RUD for secret-id-num-uses field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/secret-id-num-uses", true).await;
        assert!(resp.is_ok());

        let req_data = json!({
            "secret_id_num_uses": 200,
        })
        .as_object()
        .unwrap()
        .clone();
        let _ = test_write_api(
            &core,
            &root_token,
            "auth/approle/role/role1/secret-id-num-uses",
            true,
            Some(req_data.clone()),
        )
        .await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/secret-id-num-uses", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["secret_id_num_uses"].as_int().unwrap(), req_data["secret_id_num_uses"].as_int().unwrap());

        let _ = test_delete_api(&core, &root_token, "auth/approle/role/role1/secret-id-num-uses", true, None).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/secret-id-num-uses", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["secret_id_num_uses"].as_int().unwrap(), 0);

        // RUD for secret_id_ttl field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/secret-id-ttl", true).await;
        assert!(resp.is_ok());

        let req_data = json!({
            "secret_id_ttl": 3001,
        })
        .as_object()
        .unwrap()
        .clone();
        let _ =
            test_write_api(&core, &root_token, "auth/approle/role/role1/secret-id-ttl", true, Some(req_data.clone()))
                .await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/secret-id-ttl", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["secret_id_ttl"].as_int().unwrap(), req_data["secret_id_ttl"].as_int().unwrap());

        let _ = test_delete_api(&core, &root_token, "auth/approle/role/role1/secret-id-ttl", true, None).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/secret-id-ttl", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["secret_id_ttl"].as_int().unwrap(), 0);

        // RUD for token-num-uses field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-num-uses", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_num_uses"].as_int().unwrap(), 600);

        let req_data = json!({
            "token_num_uses": 60,
        })
        .as_object()
        .unwrap()
        .clone();
        let _ =
            test_write_api(&core, &root_token, "auth/approle/role/role1/token-num-uses", true, Some(req_data.clone()))
                .await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-num-uses", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_num_uses"].as_int().unwrap(), req_data["token_num_uses"].as_int().unwrap());

        let _ = test_delete_api(&core, &root_token, "auth/approle/role/role1/token-num-uses", true, None).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-num-uses", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_num_uses"].as_int().unwrap(), 0);

        // RUD for period field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/period", true).await;
        assert!(resp.is_ok());

        let req_data = json!({
            "period": 9001,
        })
        .as_object()
        .unwrap()
        .clone();
        let _ =
            test_write_api(&core, &root_token, "auth/approle/role/role1/period", true, Some(req_data.clone())).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/period", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["period"].as_int().unwrap(), req_data["period"].as_int().unwrap());

        let _ = test_delete_api(&core, &root_token, "auth/approle/role/role1/period", true, None).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/period", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_period"].as_int().unwrap(), 0);

        // RUD for token_ttl field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-ttl", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_ttl"].as_int().unwrap(), 4000);

        let req_data = json!({
            "token_ttl": 4001,
        })
        .as_object()
        .unwrap()
        .clone();
        let _ =
            test_write_api(&core, &root_token, "auth/approle/role/role1/token-ttl", true, Some(req_data.clone())).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-ttl", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_ttl"].as_int().unwrap(), req_data["token_ttl"].as_int().unwrap());

        let _ = test_delete_api(&core, &root_token, "auth/approle/role/role1/token-ttl", true, None).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-ttl", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_ttl"].as_int().unwrap(), 0);

        // RUD for token_max_ttl field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-max-ttl", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_max_ttl"].as_int().unwrap(), 5000);

        let req_data = json!({
            "token_max_ttl": 5001,
        })
        .as_object()
        .unwrap()
        .clone();
        let _ =
            test_write_api(&core, &root_token, "auth/approle/role/role1/token-max-ttl", true, Some(req_data.clone()))
                .await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-max-ttl", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_max_ttl"].as_int().unwrap(), req_data["token_max_ttl"].as_int().unwrap());

        let _ = test_delete_api(&core, &root_token, "auth/approle/role/role1/token-max-ttl", true, None).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-max-ttl", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_max_ttl"].as_int().unwrap(), 0);

        // Delete test for role
        test_delete_role(&core, &root_token, "approle", "role1").await;
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        assert!(resp.unwrap().is_none());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_token_bound_cidrs_crud() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_role_token_bound_cidrs_crud").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let req_data = json!({
            "policies": "p,q,r,s",
            "secret_id_num_uses": 10,
            "secret_id_ttl":      300,
            "token_ttl":          400,
            "token_max_ttl":      500,
            "token_num_uses":     600,
            "secret_id_bound_cidrs": "127.0.0.1/32,127.0.0.1/16",
            "token_bound_cidrs":     "127.0.0.1/32,127.0.0.1/16",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, req_data).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();

        let expected = json!({
            "bind_secret_id":        true,
            "local_secret_ids":      false,
            "policies":              ["p", "q", "r", "s"],
            "secret_id_num_uses":    10,
            "secret_id_ttl":         300,
            "token_ttl":             400,
            "token_max_ttl":         500,
            "token_num_uses":        600,
            "token_no_default_policy": false,
            "secret_id_bound_cidrs": ["127.0.0.1/32", "127.0.0.1/16"],
            "token_bound_cidrs":     ["127.0.0.1", "127.0.0.1/16"],
            "token_period":          0,
            "token_explicit_max_ttl":0,
            "token_policies":        ["p", "q", "r", "s"],
            "token_type":            "default",
        });
        assert_eq!(expected.as_object().unwrap().clone(), resp_data);

        let req_data = json!({
            "role_id": "test_role_id",
            "policies": "a,b,c,d",
            "secret_id_num_uses": 100,
            "secret_id_ttl":      3000,
            "token_ttl":          4000,
            "token_max_ttl":      5000,
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, req_data).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();

        let expected = json!({
            "bind_secret_id":        true,
            "local_secret_ids":      false,
            "policies":              ["a", "b", "c", "d"],
            "secret_id_num_uses":    100,
            "secret_id_ttl":         3000,
            "token_ttl":             4000,
            "token_max_ttl":         5000,
            "token_num_uses":        600,
            "token_no_default_policy": false,
            "secret_id_bound_cidrs": ["127.0.0.1/32", "127.0.0.1/16"],
            "token_period":          0,
            "token_explicit_max_ttl":0,
            "token_bound_cidrs":     ["127.0.0.1", "127.0.0.1/16"],
            "token_policies":        ["a", "b", "c", "d"],
            "token_type":            "default",
        });
        assert_eq!(expected.as_object().unwrap().clone(), resp_data);

        // RUD for secret-id-bound-cidrs field
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/secret-id-bound-cidrs", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(
            resp_data["secret_id_bound_cidrs"].as_comma_string_slice().unwrap(),
            expected["secret_id_bound_cidrs"].as_comma_string_slice().unwrap()
        );

        let req_data = json!({
            "secret_id_bound_cidrs": ["127.0.0.1/20"],
        })
        .as_object()
        .unwrap()
        .clone();
        let _ = test_write_api(
            &core,
            &root_token,
            "auth/approle/role/role1/secret-id-bound-cidrs",
            true,
            Some(req_data.clone()),
        )
        .await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/secret-id-bound-cidrs", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(
            resp_data["secret_id_bound_cidrs"].as_comma_string_slice().unwrap(),
            req_data["secret_id_bound_cidrs"].as_comma_string_slice().unwrap()
        );

        let _ = test_delete_api(&core, &root_token, "auth/approle/role/role1/secret-id-bound-cidrs", true, None).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/secret-id-bound-cidrs", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["secret_id_bound_cidrs"].as_comma_string_slice().unwrap().len(), 0);

        // RUD for token-bound-cidrs field
        let expected = json!({
            "token_bound_cidrs":     ["127.0.0.1", "127.0.0.1/16"],
        });
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-bound-cidrs", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(
            resp_data["token_bound_cidrs"].as_comma_string_slice().unwrap(),
            expected["token_bound_cidrs"].as_comma_string_slice().unwrap()
        );

        let req_data = json!({
            "token_bound_cidrs": ["127.0.0.1/20"],
        })
        .as_object()
        .unwrap()
        .clone();
        let _ = test_write_api(
            &core,
            &root_token,
            "auth/approle/role/role1/token-bound-cidrs",
            true,
            Some(req_data.clone()),
        )
        .await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-bound-cidrs", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(
            resp_data["token_bound_cidrs"].as_comma_string_slice().unwrap(),
            req_data["token_bound_cidrs"].as_comma_string_slice().unwrap()
        );

        let _ = test_delete_api(&core, &root_token, "auth/approle/role/role1/token-bound-cidrs", true, None).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1/token-bound-cidrs", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        assert_eq!(resp_data["token_bound_cidrs"].as_comma_string_slice().unwrap().len(), 0);

        // Delete test for role
        test_delete_role(&core, &root_token, "approle", "role1").await;
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        assert!(resp.unwrap().is_none());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_token_type_crud() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_role_token_type_crud").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let req_data = json!({
            "policies": "p,q,r,s",
            "secret_id_num_uses": 10,
            "secret_id_ttl":      300,
            "token_ttl":          400,
            "token_max_ttl":      500,
            "token_num_uses":     600,
            "token_type":         "default-service",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, req_data).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();

        let expected = json!({
            "bind_secret_id":        true,
            "local_secret_ids":      false,
            "policies":              ["p", "q", "r", "s"],
            "secret_id_num_uses":    10,
            "secret_id_ttl":         300,
            "token_ttl":             400,
            "token_max_ttl":         500,
            "token_num_uses":        600,
            "token_no_default_policy": false,
            "secret_id_bound_cidrs": [],
            "token_bound_cidrs":     [],
            "token_period":          0,
            "token_explicit_max_ttl":0,
            "token_policies":        ["p", "q", "r", "s"],
            "token_type":            "service",
        });
        assert_eq!(expected.as_object().unwrap().clone(), resp_data);

        let req_data = json!({
            "role_id": "test_role_id",
            "policies": "a,b,c,d",
            "secret_id_num_uses": 100,
            "secret_id_ttl":      3000,
            "token_ttl":          4000,
            "token_max_ttl":      5000,
            "token_type":         "default-service",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, req_data).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();

        let expected = json!({
            "bind_secret_id":        true,
            "local_secret_ids":      false,
            "policies":              ["a", "b", "c", "d"],
            "secret_id_num_uses":    100,
            "secret_id_ttl":         3000,
            "token_ttl":             4000,
            "token_max_ttl":         5000,
            "token_num_uses":        600,
            "token_no_default_policy": false,
            "secret_id_bound_cidrs": [],
            "token_period":          0,
            "token_explicit_max_ttl":0,
            "token_bound_cidrs":     [],
            "token_policies":        ["a", "b", "c", "d"],
            "token_type":            "service",
        });
        assert_eq!(expected.as_object().unwrap().clone(), resp_data);

        // Delete test for role
        test_delete_role(&core, &root_token, "approle", "role1").await;
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        assert!(resp.unwrap().is_none());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_token_util_upgrade() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_role_token_util_upgrade").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        // token_type missing
        let req_data = json!({
            "policies": "p,q,r,s",
            "secret_id_num_uses": 10,
            "secret_id_ttl":      300,
            "token_ttl":          400,
            "token_max_ttl":      500,
            "token_num_uses":     600,
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, req_data).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();

        let expected = json!({
            "bind_secret_id":        true,
            "local_secret_ids":      false,
            "policies":              ["p", "q", "r", "s"],
            "secret_id_num_uses":    10,
            "secret_id_ttl":         300,
            "token_ttl":             400,
            "token_max_ttl":         500,
            "token_num_uses":        600,
            "token_no_default_policy": false,
            "secret_id_bound_cidrs": [],
            "token_bound_cidrs":     [],
            "token_period":          0,
            "token_explicit_max_ttl":0,
            "token_policies":        ["p", "q", "r", "s"],
            "token_type":            "default",
        });
        assert_eq!(expected.as_object().unwrap().clone(), resp_data);

        // token_type empty
        let req_data = json!({
            "role_id": "test_role_id",
            "policies": "a,b,c,d",
            "secret_id_num_uses": 100,
            "secret_id_ttl":      3000,
            "token_ttl":          4000,
            "token_max_ttl":      5000,
            "token_type":         "",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, req_data).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();

        let expected = json!({
            "bind_secret_id":        true,
            "local_secret_ids":      false,
            "policies":              ["a", "b", "c", "d"],
            "secret_id_num_uses":    100,
            "secret_id_ttl":         3000,
            "token_ttl":             4000,
            "token_max_ttl":         5000,
            "token_num_uses":        600,
            "token_no_default_policy": false,
            "secret_id_bound_cidrs": [],
            "token_period":          0,
            "token_explicit_max_ttl":0,
            "token_bound_cidrs":     [],
            "token_policies":        ["a", "b", "c", "d"],
            "token_type":            "default",
        });
        assert_eq!(expected.as_object().unwrap().clone(), resp_data);

        // token_type service
        let req_data = json!({
            "role_id": "test_role_id",
            "policies": "a,b,c,d",
            "secret_id_num_uses": 100,
            "secret_id_ttl":      3000,
            "token_ttl":          4000,
            "token_max_ttl":      5000,
            "token_type":         "service",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &root_token, "auth/approle/role/role1", true, req_data).await;

        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        let resp_data = resp.unwrap().unwrap().data.unwrap();

        let expected = json!({
            "bind_secret_id":        true,
            "local_secret_ids":      false,
            "policies":              ["a", "b", "c", "d"],
            "secret_id_num_uses":    100,
            "secret_id_ttl":         3000,
            "token_ttl":             4000,
            "token_max_ttl":         5000,
            "token_num_uses":        600,
            "token_no_default_policy": false,
            "secret_id_bound_cidrs": [],
            "token_period":          0,
            "token_explicit_max_ttl":0,
            "token_bound_cidrs":     [],
            "token_policies":        ["a", "b", "c", "d"],
            "token_type":            "service",
        });
        assert_eq!(expected.as_object().unwrap().clone(), resp_data);

        // Delete test for role
        test_delete_role(&core, &root_token, "approle", "role1").await;
        let resp = test_read_api(&core, &root_token, "auth/approle/role/role1", true).await;
        assert!(resp.unwrap().is_none());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_secret_id_with_ttl() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_approle_role_secret_id_with_ttl").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        let mut role_data = json!({
            "policies": "default",
            "secret_id_ttl":      0,
        })
        .as_object()
        .unwrap()
        .clone();

        let cases = vec![
            json!({"name": "zero ttl", "role_name": "role-zero-ttl", "ttl": 0, "sys_ttl_cap": false}),
            json!({"name": "custom ttl", "role_name": "role-custom-ttl", "ttl": 60, "sys_ttl_cap": false}),
            json!({"name": "system ttl capped", "role_name": "role-sys-ttl-cap", "ttl": 700000000, "sys_ttl_cap": true}),
        ];

        for case in cases.iter() {
            let role_name = case["role_name"].as_str().unwrap();
            role_data["secret_id_ttl"] = case["ttl"].clone();
            let _ = test_write_api(
                &core,
                &root_token,
                format!("auth/approle/role/{}", role_name).as_str(),
                true,
                Some(role_data.clone()),
            )
            .await;

            let resp = test_write_api(
                &core,
                &root_token,
                format!("auth/approle/role/{}/secret-id", role_name).as_str(),
                true,
                None,
            )
            .await;
            assert!(resp.is_ok());
            let resp_data = resp.unwrap().unwrap().data.unwrap();
            let secret_id_ttl = resp_data["secret_id_ttl"].as_duration().unwrap();
            if case["sys_ttl_cap"].as_bool().unwrap() {
                assert_eq!(secret_id_ttl, MAX_LEASE_DURATION_SECS);
            } else {
                assert_eq!(secret_id_ttl, case["ttl"].as_duration().unwrap());
            }
        }
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_approle_role_secret_id_accessor_cross_delete() {
        let (_rvault, core, root_token) =
            new_unseal_test_rusty_vault("test_approle_role_secret_id_accessor_cross_delete").await;

        // Mount approle auth to path: auth/approle
        test_mount_auth_api(&core, &root_token, "approle", "approle").await;

        // Create First Role
        test_write_role(&core, &root_token, "approle", "role1", "", "a,b", true).await;
        let _ = generate_secret_id(&core, &root_token, "approle", "role1").await;

        // Create Second Role
        test_write_role(&core, &root_token, "approle", "role2", "", "a,b", true).await;
        let _ = generate_secret_id(&core, &root_token, "approle", "role2").await;

        // Get role2 secretID Accessor
        let resp = test_list_api(&core, &root_token, "auth/approle/role/role2/secret-id", true).await;
        assert!(resp.is_ok());
        let resp_data = resp.unwrap().unwrap().data.unwrap();
        let keys = resp_data["keys"].as_array().unwrap();
        assert_eq!(keys.len(), 1);

        // Attempt to destroy role2 secretID accessor using role1 path

        let hmac_secret_id = keys[0].as_str().unwrap();
        let hmac_data = json!({
            "secret_id_accessor": hmac_secret_id,
        })
        .as_object()
        .cloned();
        let _ =
            test_delete_api(&core, &root_token, "auth/approle/role/role1/secret-id-accessor/destroy", false, hmac_data)
                .await;
    }
}
