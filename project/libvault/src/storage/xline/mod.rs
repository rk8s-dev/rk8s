use crate::errors::RvError;
use crate::storage::{Backend, BackendEntry};
use etcd_client::{Client, ConnectOptions, GetOptions, KvClient, TlsOptions};
use itertools::Itertools;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::OnceCell;
use tonic::transport::{Certificate, Identity};

pub struct XlineBackend {
    client: OnceCell<Client>,
    option: XlineOptions,
}

#[derive(Clone)]
pub struct XlineOptions {
    pub endpoints: Vec<String>,
    pub config: Option<ConnectOptions>,
}

impl XlineOptions {
    pub fn new(endpoints: Vec<String>) -> Self {
        Self {
            endpoints,
            config: None,
        }
    }

    pub fn with_tls(
        mut self,
        root_cert: impl AsRef<str>,
        cert: impl AsRef<str>,
        private_key: impl AsRef<str>,
    ) -> anyhow::Result<Self> {
        let tls_cfg = TlsOptions::default()
            .ca_certificate(Certificate::from_pem(root_cert.as_ref()))
            .identity(Identity::from_pem(cert.as_ref(), private_key.as_ref()));

        self.config = Some(ConnectOptions::default().with_tls(tls_cfg));
        Ok(self)
    }
}

impl XlineBackend {
    pub fn new(conf: &HashMap<String, Value>) -> Result<Self, RvError> {
        let endpoints = conf
            .get("endpoints")
            .and_then(|v| v.as_array())
            .and_then(|v| {
                v.iter()
                    .map(|e| e.as_str().map(|e| e.to_string()))
                    .collect::<Option<Vec<_>>>()
            })
            .ok_or(RvError::ErrDatabaseConnectionInfoInvalid)?;

        Ok(Self {
            client: OnceCell::new(),
            option: XlineOptions::new(endpoints),
        })
    }

    pub fn with_options(option: XlineOptions) -> Self {
        Self {
            client: OnceCell::new(),
            option,
        }
    }

    pub async fn get_kv_client_or_try_init(&self) -> Result<KvClient, RvError> {
        let client = self
            .client
            .get_or_try_init(|| async {
                let client =
                    Client::connect(&self.option.endpoints, self.option.config.clone()).await?;
                Ok::<_, RvError>(client)
            })
            .await?;
        Ok(client.kv_client())
    }
}

#[async_trait::async_trait]
impl Backend for XlineBackend {
    async fn list(&self, prefix: &str) -> Result<Vec<String>, RvError> {
        if prefix.starts_with("/") {
            return Err(RvError::ErrPhysicalBackendPrefixInvalid);
        }

        let mut client = self.get_kv_client_or_try_init().await?;

        let resp = client
            .get(prefix, Some(GetOptions::default().with_prefix()))
            .await?;
        Ok(resp
            .kvs()
            .iter()
            .map(|e| {
                let key = String::from_utf8_lossy(e.key());
                let key = key.trim_start_matches(prefix);

                match key.find("/") {
                    Some(idx) => &key[0..idx + 1],
                    None => key,
                }
                .to_string()
            })
            .unique()
            .collect())
    }

    async fn get(&self, key: &str) -> Result<Option<BackendEntry>, RvError> {
        let mut client = self.get_kv_client_or_try_init().await?;
        let resp = client.get(key, None).await?;

        Ok(resp.kvs().first().map(|e| {
            let key = String::from_utf8_lossy(e.key()).to_string();
            BackendEntry {
                key,
                value: e.value().to_vec(),
            }
        }))
    }

    async fn put(&self, entry: &BackendEntry) -> Result<(), RvError> {
        let mut client = self.get_kv_client_or_try_init().await?;
        client.put(&*entry.key, &*entry.value, None).await?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), RvError> {
        let mut client = self.get_kv_client_or_try_init().await?;
        client.delete(key, None).await?;
        Ok(())
    }
}
