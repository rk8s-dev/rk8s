use std::{collections::HashMap, fs, io::BufReader, path::PathBuf, sync::Arc, time::Duration};

use better_default::Default;
use rustls::{
    pki_types::{pem::PemObject, PrivateKeyDer},
    ClientConfig, RootCertStore, ALL_VERSIONS,
};
use serde_json::{Map, Value};
use ureq::AgentBuilder;
use webpki_roots::TLS_SERVER_ROOTS;

use super::HttpResponse;
use crate::{errors::RvError, utils::cert::DisabledVerifier};

#[derive(Clone)]
pub struct TLSConfig {
    client_config: ClientConfig,
}

#[derive(Default, Clone)]
pub struct TLSConfigBuilder {
    pub server_ca_pem: Option<Vec<u8>>,
    pub client_cert_pem: Option<Vec<u8>>,
    pub client_key_pem: Option<Vec<u8>>,
    pub tls_server_name: Option<String>,
    pub insecure: bool,
}

#[derive(Default, Clone)]
pub struct Client {
    #[default("https://127.0.0.1:8200".into())]
    pub address: String,
    pub token: String,
    #[default(HashMap::new())]
    pub headers: HashMap<String, String>,
    pub tls_config: Option<TLSConfig>,
    #[default(ureq::Agent::new())]
    pub http_client: ureq::Agent,
}

impl TLSConfigBuilder {
    pub fn new() -> Self {
        TLSConfigBuilder::default()
    }

    pub fn with_server_ca_path(mut self, server_ca_path: &PathBuf) -> Result<Self, RvError> {
        let cert_data = fs::read(server_ca_path)?;
        self.server_ca_pem = Some(cert_data);
        Ok(self)
    }

    pub fn with_server_ca_pem(mut self, server_ca_pem: &str) -> Self {
        self.server_ca_pem = Some(server_ca_pem.as_bytes().to_vec());
        self
    }

    pub fn with_client_cert_path(
        mut self,
        client_cert_path: &PathBuf,
        client_key_path: &PathBuf,
    ) -> Result<Self, RvError> {
        let cert_data = fs::read(client_cert_path)?;
        self.client_cert_pem = Some(cert_data);

        let key_data = fs::read(client_key_path)?;
        self.client_key_pem = Some(key_data);

        Ok(self)
    }

    pub fn with_client_cert_pem(mut self, client_cert_pem: &str, client_key_pem: &str) -> Self {
        self.client_cert_pem = Some(client_cert_pem.as_bytes().to_vec());
        self.client_key_pem = Some(client_key_pem.as_bytes().to_vec());

        self
    }

    pub fn with_insecure(mut self, insecure: bool) -> Self {
        self.insecure = insecure;

        self
    }

    pub fn build(self) -> Result<TLSConfig, RvError> {
        let provider = rustls::crypto::CryptoProvider::get_default()
            .cloned()
            .unwrap_or(Arc::new(rustls::crypto::ring::default_provider()));

        let builder = ClientConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(ALL_VERSIONS)
            .expect("all TLS versions");

        let builder = if self.insecure {
            log::debug!("Certificate verification disabled");
            builder.dangerous().with_custom_certificate_verifier(Arc::new(DisabledVerifier))
        } else if let Some(server_ca) = &self.server_ca_pem {
            let mut cert_reader = BufReader::new(&server_ca[..]);
            let root_certs = rustls_pemfile::certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;

            let mut root_store = RootCertStore::empty();
            let (_added, _ignored) = root_store.add_parsable_certificates(root_certs);
            builder.with_root_certificates(root_store)
        } else {
            let root_store = RootCertStore { roots: TLS_SERVER_ROOTS.to_vec() };
            builder.with_root_certificates(root_store)
        };

        let client_config =
            if let (Some(client_cert_pem), Some(client_key_pem)) = (&self.client_cert_pem, &self.client_key_pem) {
                let mut cert_reader = BufReader::new(&client_cert_pem[..]);
                let client_certs = rustls_pemfile::certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;
                let client_key = PrivateKeyDer::from_pem_slice(client_key_pem)?;

                builder.with_client_auth_cert(client_certs, client_key)?
            } else {
                builder.with_no_client_auth()
            };

        Ok(TLSConfig { client_config })
    }
}

impl Client {
    pub fn new() -> Self {
        Client::default()
    }

    pub fn with_addr(mut self, addr: &str) -> Self {
        self.address = addr.into();
        self
    }

    pub fn with_token(mut self, token: &str) -> Self {
        self.token = token.into();
        self
    }

    pub fn with_tls_config(mut self, tls_config: TLSConfig) -> Self {
        self.tls_config = Some(tls_config);
        self
    }

    pub fn add_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn build(mut self) -> Self {
        let mut agent = AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(30));

        if let Some(tls_config) = &self.tls_config {
            agent = agent.tls_config(Arc::new(tls_config.client_config.clone()));
        }

        self.http_client = agent.build();
        self
    }

    pub fn request<S: Into<String>>(
        &self,
        method: &str,
        path: S,
        data: Option<Map<String, Value>>,
    ) -> Result<HttpResponse, RvError> {
        let path = path.into();
        let url = if path.starts_with('/') {
            format!("{}{}", self.address, path)
        } else {
            format!("{}/{}", self.address, path)
        };
        log::debug!("request url: {url}, method: {method}");

        let mut req = self.http_client.request(&method.to_uppercase(), &url);

        req = req.set("Accept", "application/json");
        if !path.ends_with("/login") {
            req = req.set("X-RustyVault-Token", &self.token);
        }

        let mut ret = HttpResponse { method: method.to_string(), url, ..Default::default() };

        let response_result = if let Some(send_data) = data { req.send_json(send_data) } else { req.call() };

        match response_result {
            Ok(response) => {
                ret.response_status = response.status();
                if ret.response_status == 204 {
                    return Ok(ret.clone());
                }
                let json: Value = response.into_json()?;
                ret.response_data = Some(json);
                Ok(ret.clone())
            }
            Err(ureq::Error::Status(status, response)) => {
                ret.response_status = status;
                if let Ok(response_data) = response.into_json() {
                    ret.response_data = Some(response_data);
                }
                Ok(ret.clone())
            }
            Err(e) => {
                log::error!("Request failed: {e}");
                Err(RvError::UreqError { source: e })
            }
        }
    }

    pub fn request_list<S: Into<String>>(&self, path: S) -> Result<HttpResponse, RvError> {
        self.request("LIST", path, None)
    }

    pub fn request_read<S: Into<String>>(&self, path: S) -> Result<HttpResponse, RvError> {
        self.request("GET", path, None)
    }

    pub fn request_get<S: Into<String>>(&self, path: S) -> Result<HttpResponse, RvError> {
        self.request("GET", path, None)
    }

    pub fn request_write<S: Into<String>>(
        &self,
        path: S,
        data: Option<Map<String, Value>>,
    ) -> Result<HttpResponse, RvError> {
        self.request("POST", path, data)
    }

    pub fn request_put<S: Into<String>>(
        &self,
        path: S,
        data: Option<Map<String, Value>>,
    ) -> Result<HttpResponse, RvError> {
        self.request("PUT", path, data)
    }

    pub fn request_delete<S: Into<String>>(
        &self,
        path: S,
        data: Option<Map<String, Value>>,
    ) -> Result<HttpResponse, RvError> {
        self.request("DELETE", path, data)
    }
}
