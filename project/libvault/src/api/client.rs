use std::{collections::HashMap, time::Duration};

use reqwest::{Client as ReqwestClient, Method};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::HttpResponse;
use crate::{
    api::{
        tls::{TLSConfig, TLSConfigBuilder},
        types::ApiResponse,
    },
    errors::RvError,
    http::AUTH_HEADER_NAME,
};

#[derive(Clone)]
pub struct Client {
    address: String,
    token: String,
    headers: HashMap<String, String>,
    http_client: ReqwestClient,
}

#[derive(Default)]
pub struct ClientBuilder {
    address: String,
    token: String,
    headers: HashMap<String, String>,
    tls_config: Option<TLSConfig>,
}

impl Client {
    pub fn builder() -> ClientBuilder {
        ClientBuilder {
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

    pub fn clone_with_token(&self, token: &str) -> Self {
        let mut cloned = self.clone();
        cloned.token = token.into();
        cloned
    }

    async fn request_http<S: Into<String>, T: Serialize>(
        &self,
        method: &str,
        path: S,
        data: Option<T>,
    ) -> Result<HttpResponse, RvError> {
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

        let response = match req.send().await {
            Ok(resp) => resp,
            Err(err) => panic!("async HTTP request failed: {err:?}"),
        };
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

        Ok(http_resp)
    }

    pub async fn request_raw<S: Into<String>, T: Serialize>(
        &self,
        method: &str,
        path: S,
        data: Option<T>,
    ) -> Result<HttpResponse, RvError> {
        self.request_http(method, path, data).await
    }

    pub async fn request<S: Into<String>, T: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        path: S,
        data: Option<T>,
    ) -> Result<ApiResponse<R>, RvError> {
        let http_resp = self.request_http(method, path, data).await?;
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

impl ClientBuilder {
    pub fn with_addr(mut self, addr: impl Into<String>) -> Self {
        self.address = addr.into();
        self
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = token.into();
        self
    }

    pub fn add_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
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

    pub fn build(self) -> Result<Client, RvError> {
        let mut builder = ReqwestClient::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30));

        if let Some(tls_config) = &self.tls_config {
            if tls_config.insecure() {
                builder = builder.danger_accept_invalid_certs(true);
            }

            if let Some(ca_pem) = tls_config.server_ca_pem() {
                let cert = reqwest::tls::Certificate::from_pem(ca_pem)?;
                builder = builder.add_root_certificate(cert);
            }

            if let (Some(cert_pem), Some(key_pem)) =
                (tls_config.client_cert_pem(), tls_config.client_key_pem())
            {
                let mut identity_pem = Vec::new();
                identity_pem.extend_from_slice(cert_pem);
                if !identity_pem.ends_with(b"\n") {
                    identity_pem.push(b'\n');
                }
                identity_pem.extend_from_slice(key_pem);
                let identity = reqwest::Identity::from_pem(&identity_pem)?;
                builder = builder.identity(identity);
            }
        }

        let http_client = match builder.build() {
            Ok(client) => client,
            Err(err) => panic!("failed to build async HTTP client: {err:?}"),
        };

        Ok(Client {
            address: self.address,
            token: self.token,
            headers: self.headers,
            http_client,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Client;

    #[test]
    fn builder_methods_do_not_panic() {
        let client = Client::builder()
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
