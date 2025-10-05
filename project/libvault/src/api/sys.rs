use std::time::Duration;

use derive_more::Deref;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::{Client, HttpResponse, secret::SecretAuth};
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
    #[serde(
        default,
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub default_lease_ttl: Duration,
    #[serde(
        default,
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
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
    pub async fn init(&self, init_req: &InitRequest) -> Result<HttpResponse, RvError> {
        let data = json!({
            "secret_shares": init_req.secret_shares,
            "secret_threshold": init_req.secret_threshold,
        });

        self.client
            .request_raw("PUT", "/v1/sys/init", data.as_object().cloned())
            .await
    }

    pub async fn seal_status(&self) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, Value>("GET", "/v1/sys/seal-status", None::<Value>)
            .await
    }

    pub async fn seal(&self) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, Value>("PUT", "/v1/sys/seal", None::<Value>)
            .await
    }

    pub async fn unseal(&self, key: &str) -> Result<HttpResponse, RvError> {
        let data = json!({
            "key": key,
        });

        self.client
            .request_raw("PUT", "/v1/sys/unseal", data.as_object().cloned())
            .await
    }

    pub async fn list_auth(&self) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, Value>("GET", "/v1/sys/auth", None::<Value>)
            .await
    }

    pub async fn enable_auth(
        &self,
        path: &str,
        input: &AuthInput,
    ) -> Result<HttpResponse, RvError> {
        let data = serde_json::to_value(input)?;
        self.client
            .request_raw(
                "POST",
                format!("/v1/sys/auth/{path}"),
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn disable_auth(&self, path: &str) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, Value>("DELETE", format!("/v1/sys/auth/{path}"), None::<Value>)
            .await
    }

    pub async fn mount(&self, path: &str, input: &MountInput) -> Result<HttpResponse, RvError> {
        let data = serde_json::to_value(input)?;
        self.client
            .request_raw(
                "POST",
                format!("/v1/sys/mounts/{path}"),
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn unmount(&self, path: &str) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, Value>("DELETE", format!("/v1/sys/mounts/{path}"), None::<Value>)
            .await
    }

    pub async fn remount(&self, from: &str, to: &str) -> Result<HttpResponse, RvError> {
        let data = json!({
            "from": from,
            "to": to,
        });

        self.client
            .request_raw("POST", "/v1/sys/remount", data.as_object().cloned())
            .await
    }

    pub async fn list_mounts(&self) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, Value>("GET", "/v1/sys/mounts", None::<Value>)
            .await
    }

    pub async fn list_policy(&self) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, Value>("GET", "/v1/sys/policies/acl", None::<Value>)
            .await
    }

    pub async fn read_policy(&self, name: &str) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, Value>("GET", format!("/v1/sys/policies/acl/{name}"), None::<Value>)
            .await
    }

    pub async fn write_policy(&self, name: &str, policy: &str) -> Result<HttpResponse, RvError> {
        let data = json!({
            "policy": policy,
        });

        self.client
            .request_raw(
                "POST",
                format!("/v1/sys/policies/acl/{name}"),
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn delete_policy(&self, name: &str) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, Value>(
                "DELETE",
                format!("/v1/sys/policies/acl/{name}"),
                None::<Value>,
            )
            .await
    }
}
