use std::{collections::HashMap, sync::Arc, time::Duration};

use reqwest::{Client as ReqwestClient, Method};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::HttpResponse;
use crate::{
    api::client::{TLSConfig, TLSConfigBuilder},
    api::types::ApiResponse,
    errors::RvError,
    http::AUTH_HEADER_NAME,
};

#[derive(Clone)]
pub struct AsyncClient {
    address: String,
    token: String,
    headers: HashMap<String, String>,
    http_client: ReqwestClient,
}

#[derive(Default)]
pub struct AsyncClientBuilder {
    address: String,
    token: String,
    headers: HashMap<String, String>,
    tls_config: Option<TLSConfig>,
}

impl AsyncClient {
    pub fn builder() -> AsyncClientBuilder {
        AsyncClientBuilder {
            address: "https://127.0.0.1:8200".into(),
            token: String::new(),
            headers: HashMap::new(),
            tls_config: None,
        }
    }

    pub fn new() -> Self {
        Self::builder()
            .build()
            .expect("failed to construct default async client")
    }

    pub async fn request<S: Into<String>, T: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        path: S,
        data: Option<T>,
    ) -> Result<ApiResponse<R>, RvError> {
        let path = path.into();
        let url = if path.starts_with('/') {
            format!("{}{}", self.address, path)
        } else {
            format!("{}/{}", self.address, path)
        };

        let req_method = match method.to_ascii_uppercase().as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            "LIST" => Method::from_bytes(b"LIST").expect("LIST method"),
            other => Method::from_bytes(other.as_bytes()).expect("http method"),
        };

        let mut req = self.http_client.request(req_method, &url);
        req = req.header("Accept", "application/json");

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v);
        }

        let skip_token = path.ends_with("/login");
        if !skip_token && !self.token.is_empty() {
            req = req.header(AUTH_HEADER_NAME, &self.token);
        }

        if let Some(body) = data {
            req = req.json(&body);
        }

        let response = req.send().await?;
        let status = response.status();
        let mut http_resp = HttpResponse {
            method: method.to_string(),
            url,
            response_status: status.as_u16(),
            response_data: None,
        };

        if status != reqwest::StatusCode::NO_CONTENT {
            let bytes = response.bytes().await?;
            if !bytes.is_empty() {
                http_resp.response_data = Some(serde_json::from_slice(&bytes)?);
            }
        }

        let api_resp = http_resp.parse::<R>()?;
        Ok(api_resp)
    }

    pub async fn request_list<S: Into<String>, R: DeserializeOwned>(
        &self,
        path: S,
    ) -> Result<ApiResponse<R>, RvError> {
        self.request::<_, (), R>("LIST", path, None).await
    }

    pub async fn request_read<S: Into<String>, R: DeserializeOwned>(
        &self,
        path: S,
    ) -> Result<ApiResponse<R>, RvError> {
        self.request::<_, (), R>("GET", path, None).await
    }

    pub async fn request_get<S: Into<String>, R: DeserializeOwned>(
        &self,
        path: S,
    ) -> Result<ApiResponse<R>, RvError> {
        self.request::<_, (), R>("GET", path, None).await
    }

    pub async fn request_write<S: Into<String>, T: Serialize, R: DeserializeOwned>(
        &self,
        path: S,
        data: Option<T>,
    ) -> Result<ApiResponse<R>, RvError> {
        self.request("POST", path, data).await
    }

    pub async fn request_put<S: Into<String>, T: Serialize, R: DeserializeOwned>(
        &self,
        path: S,
        data: Option<T>,
    ) -> Result<ApiResponse<R>, RvError> {
        self.request("PUT", path, data).await
    }

    pub async fn request_delete<S: Into<String>, R: DeserializeOwned>(
        &self,
        path: S,
    ) -> Result<ApiResponse<R>, RvError> {
        self.request::<_, (), R>("DELETE", path, None).await
    }
}

impl AsyncClientBuilder {
    pub fn with_addr(mut self, addr: &str) -> Self {
        self.address = addr.into();
        self
    }

    pub fn with_token(mut self, token: &str) -> Self {
        self.token = token.into();
        self
    }

    pub fn add_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn with_tls_config(mut self, tls_config: TLSConfig) -> Self {
        self.tls_config = Some(tls_config);
        self
    }

    pub fn with_tls_config_builder(mut self, builder: TLSConfigBuilder) -> Result<Self, RvError> {
        self.tls_config = Some(builder.build()?);
        Ok(self)
    }

    pub fn build(self) -> Result<AsyncClient, RvError> {
        let mut builder = ReqwestClient::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30));

        if let Some(tls_config) = &self.tls_config {
            builder = builder.use_preconfigured_tls(Arc::new(tls_config.clone_inner()));
        }

        let http_client = builder.build()?;

        Ok(AsyncClient {
            address: self.address,
            token: self.token,
            headers: self.headers,
            http_client,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AsyncClient;

    #[test]
    fn builder_methods_do_not_panic() {
        let client = AsyncClient::builder()
            .with_addr("http://localhost:8200")
            .with_token("root")
            .add_header("X-Test", "1")
            .build()
            .unwrap();
        assert_eq!(client.address, "http://localhost:8200");
        assert_eq!(client.token, "root");
        assert!(client.headers.contains_key("X-Test"));
    }
}
