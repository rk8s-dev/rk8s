use std::collections::HashMap;

use derive_more::Deref;
use serde::{Deserialize, Serialize};

use super::{Client, HttpResponse};
use crate::errors::RvError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub policies: Vec<String>,
    #[serde(default)]
    pub meta: HashMap<String, String>,
    #[serde(default)]
    pub lease: String,
    #[serde(default)]
    pub ttl: String,
    #[serde(default)]
    pub explicit_max_ttl: String,
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub no_parent: bool,
    #[serde(default)]
    pub no_default_policy: bool,
    pub display_name: String,
    pub num_uses: u32,
    #[serde(default)]
    pub renewable: bool,
    #[serde(default, rename = "type")]
    pub logical_type: String,
}

#[derive(Deref)]
pub struct TokenAuth<'a> {
    #[deref]
    pub client: &'a Client,
}

impl Client {
    pub fn token(&self) -> TokenAuth<'_> {
        TokenAuth { client: self }
    }
}

impl TokenAuth<'_> {
    pub async fn create(&self, input: &TokenInput) -> Result<HttpResponse, RvError> {
        let data = serde_json::to_value(input)?;
        self.client
            .request_raw("POST", "/v1/auth/token/create", data.as_object().cloned())
            .await
    }

    pub async fn create_orphan(&self, input: &TokenInput) -> Result<HttpResponse, RvError> {
        let data = serde_json::to_value(input)?;
        self.client
            .request_raw(
                "POST",
                "/v1/auth/token/create-orphan",
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn create_with_role(
        &self,
        input: &TokenInput,
        role_name: &str,
    ) -> Result<HttpResponse, RvError> {
        let data = serde_json::to_value(input)?;
        self.client
            .request_raw(
                "POST",
                format!("/v1/auth/token/create/{role_name}"),
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn lookup(&self, token: &str) -> Result<HttpResponse, RvError> {
        let data = serde_json::json!({
            "token": token,
        });
        self.client
            .request_raw("POST", "/v1/auth/token/lookup", data.as_object().cloned())
            .await
    }

    pub async fn lookup_accessor(&self, accessor: &str) -> Result<HttpResponse, RvError> {
        let data = serde_json::json!({
            "accessor": accessor,
        });
        self.client
            .request_raw(
                "POST",
                "/v1/auth/token/lookup-accessor",
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn lookup_self(&self) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, serde_json::Value>(
                "GET",
                "/v1/auth/token/lookup-self",
                None::<serde_json::Value>,
            )
            .await
    }

    pub async fn renew(&self, token: &str, increment: u32) -> Result<HttpResponse, RvError> {
        let data = serde_json::json!({
            "token": token,
            "increment": increment,
        });
        self.client
            .request_raw("POST", "/v1/auth/token/renew", data.as_object().cloned())
            .await
    }

    pub async fn renew_accessor(
        &self,
        accessor: &str,
        increment: u32,
    ) -> Result<HttpResponse, RvError> {
        let data = serde_json::json!({
            "accessor": accessor,
            "increment": increment,
        });
        self.client
            .request_raw(
                "POST",
                "/v1/auth/token/renew-accessor",
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn renew_self(&self, increment: u32) -> Result<HttpResponse, RvError> {
        let data = serde_json::json!({
            "increment": increment,
        });
        self.client
            .request_raw(
                "POST",
                "/v1/auth/token/renew-self",
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn renew_token_as_self(
        &self,
        token: &str,
        increment: u32,
    ) -> Result<HttpResponse, RvError> {
        let client = self.client.clone_with_token(token);
        let data = serde_json::json!({
            "increment": increment,
        });
        client
            .request_raw(
                "POST",
                "/v1/auth/token/renew-self",
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn revoke_accessor(&self, accessor: &str) -> Result<HttpResponse, RvError> {
        let data = serde_json::json!({
            "accessor": accessor,
        });
        self.client
            .request_raw(
                "POST",
                "/v1/auth/token/revoke-accessor",
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn revoke_orphan(&self, token: &str) -> Result<HttpResponse, RvError> {
        let data = serde_json::json!({
            "token": token,
        });
        self.client
            .request_raw(
                "PUT",
                "/v1/auth/token/revoke-orphan",
                data.as_object().cloned(),
            )
            .await
    }

    pub async fn revoke_self(&self) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, serde_json::Value>(
                "PUT",
                "/v1/auth/token/revoke-self",
                None::<serde_json::Value>,
            )
            .await
    }

    pub async fn revoke_tree(&self, token: &str) -> Result<HttpResponse, RvError> {
        let data = serde_json::json!({
            "token": token,
        });
        self.client
            .request_raw("PUT", "/v1/auth/token/revoke", data.as_object().cloned())
            .await
    }
}
