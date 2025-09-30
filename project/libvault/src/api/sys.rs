use std::time::Duration;

use derive_more::Deref;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::{secret::SecretAuth, Client, HttpResponse};
use crate::{
    errors::RvError,
    http::sys::InitRequest,
    utils::{deserialize_duration, serialize_duration},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub lease_id: String,
    #[serde(default)]
    pub lease_duration: u32,
    #[serde(default)]
    pub renewable: bool,
    #[serde(default)]
    pub data: Map<String, Value>,
    #[serde(default)]
    pub auth: Option<SecretAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountOutput {
    #[serde(default)]
    pub uuid: String,
    #[serde(default, rename = "type")]
    pub logical_type: String,
    #[serde(default)]
    pub accessor: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub plugin_version: String,
}

pub type AuthInput = MountInput;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MountInput {
    #[serde(default, rename = "type")]
    pub logical_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub config: MountConfigInput,
    #[serde(default)]
    pub options: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MountConfigInput {
    #[serde(default, serialize_with = "serialize_duration", deserialize_with = "deserialize_duration")]
    pub default_lease_ttl: Duration,
    #[serde(default, serialize_with = "serialize_duration", deserialize_with = "deserialize_duration")]
    pub max_lease_ttl: Duration,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub options: Map<String, Value>,
}

#[derive(Deref)]
pub struct Sys<'a> {
    #[deref]
    pub client: &'a Client,
}

impl Client {
    pub fn sys(&self) -> Sys<'_> {
        Sys { client: self }
    }
}

impl Sys<'_> {
    pub fn init(&self, init_req: &InitRequest) -> Result<HttpResponse, RvError> {
        let data = json!({
            "secret_shares": init_req.secret_shares,
            "secret_threshold": init_req.secret_threshold,
        });

        self.request_put("/v1/sys/init", data.as_object().cloned())
    }

    pub fn seal_status(&self) -> Result<HttpResponse, RvError> {
        self.request_read("/v1/sys/seal-status")
    }

    pub fn seal(&self) -> Result<HttpResponse, RvError> {
        self.request_put("/v1/sys/seal", None)
    }

    pub fn unseal(&self, key: &str) -> Result<HttpResponse, RvError> {
        let data = json!({
            "key": key,
        });

        self.request_put("/v1/sys/unseal", data.as_object().cloned())
    }

    pub fn list_auth(&self) -> Result<HttpResponse, RvError> {
        self.request_read("/v1/sys/auth")
    }

    pub fn enable_auth(&self, path: &str, input: &AuthInput) -> Result<HttpResponse, RvError> {
        let data = serde_json::to_value(input)?;
        self.request_write(format!("/v1/sys/auth/{path}"), data.as_object().cloned())
    }

    pub fn disable_auth(&self, path: &str) -> Result<HttpResponse, RvError> {
        self.request_delete(format!("/v1/sys/auth/{path}"), None)
    }

    pub fn mount(&self, path: &str, input: &MountInput) -> Result<HttpResponse, RvError> {
        let data = serde_json::to_value(input)?;
        self.request_write(format!("/v1/sys/mounts/{path}"), data.as_object().cloned())
    }

    pub fn unmount(&self, path: &str) -> Result<HttpResponse, RvError> {
        self.request_delete(format!("/v1/sys/mounts/{path}"), None)
    }

    pub fn remount(&self, from: &str, to: &str) -> Result<HttpResponse, RvError> {
        let data = json!({
            "from": from,
            "to": to,
        });

        self.request_write("/v1/sys/remount", data.as_object().cloned())
    }

    pub fn list_mounts(&self) -> Result<HttpResponse, RvError> {
        self.request_read("/v1/sys/mounts")
    }

    pub fn list_policy(&self) -> Result<HttpResponse, RvError> {
        self.request_read("/v1/sys/policies/acl")
    }

    pub fn read_policy(&self, name: &str) -> Result<HttpResponse, RvError> {
        self.request_read(format!("/v1/sys/policies/acl/{name}"))
    }

    pub fn write_policy(&self, name: &str, policy: &str) -> Result<HttpResponse, RvError> {
        let data = json!({
            "policy": policy,
        });

        self.request_write(format!("/v1/sys/policies/acl/{name}"), data.as_object().cloned())
    }

    pub fn delete_policy(&self, name: &str) -> Result<HttpResponse, RvError> {
        self.request_delete(format!("/v1/sys/policies/acl/{name}"), None)
    }
}
