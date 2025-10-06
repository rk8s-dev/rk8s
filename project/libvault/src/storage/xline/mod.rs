use crate::errors::RvError;
use crate::storage::{Backend, BackendEntry};
use etcd_client::{Client, GetOptions, KvClient};
use itertools::Itertools;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::OnceCell;

pub struct XlineBackend {
    client: OnceCell<Client>,
    endpoints: Vec<String>,
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
            endpoints,
        })
    }

    pub async fn get_kv_client_or_try_init(&self) -> Result<KvClient, RvError> {
        let client = self
            .client
            .get_or_try_init(|| async {
                let client = Client::connect(&self.endpoints, None).await?;
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
