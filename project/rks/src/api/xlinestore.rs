use anyhow::Result;
use etcd_client::{Client, GetOptions, PutOptions};
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::sync::Arc;
use tokio::sync::RwLock;
/// like etcd, k:/registry/pods/pod_name v:yaml file of pod
/// k:/registry/nodes/node_name v:yaml file of node
#[derive(Clone)]
pub struct XlineStore {
    client: Arc<RwLock<Client>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeInfo {
    pub ip: String,
    pub status: String,
}

impl XlineStore {
    pub async fn new(endpoints: &[&str]) -> Result<Self> {
        let client = Client::connect(endpoints, None).await?;
        Ok(Self {
            client: Arc::new(RwLock::new(client)),
        })
    }

    pub async fn insert_pod_yaml(&self, pod_name: &str, pod_yaml: &str) -> Result<()> {
        let key = format!("/registry/pods/{}", pod_name);
        let mut client = self.client.write().await;
        client.put(key, pod_yaml, None).await?;
        Ok(())
    }

    pub async fn get_pod_yaml(&self, pod_name: &str) -> Result<Option<String>> {
        let key = format!("/registry/pods/{}", pod_name);
        let mut client = self.client.write().await;
        let resp = client.get(key, None).await?;
        Ok(resp
            .kvs()
            .first()
            .map(|kv| String::from_utf8_lossy(kv.value()).to_string()))
    }

    pub async fn list_pods(&self) -> Result<Vec<String>> {
        let mut client = self.client.write().await;
        let resp = client
            .get("/registry/pods/", Some(GetOptions::new().with_prefix()))
            .await?;
        Ok(resp
            .kvs()
            .iter()
            .map(|kv| String::from_utf8_lossy(kv.key()).replace("/registry/pods/", ""))
            .collect())
    }

    pub async fn insert_node_info(&self, node_name: &str, ip: &str, status: &str) -> Result<()> {
        let key = format!("/registry/nodes/{}", node_name);
        let value = serde_yaml::to_string(&NodeInfo {
            ip: ip.to_string(),
            status: status.to_string(),
        })?;
        let mut client = self.client.write().await;
        client.put(key, value, Some(PutOptions::new())).await?;
        Ok(())
    }

    // pub async fn get_node_info(&self, node_name: &str) -> Result<Option<NodeInfo>> {
    //     let key = format!("/registry/nodes/{}", node_name);
    //     let mut client = self.client.write().await;
    //     let resp = client.get(key, None).await?;
    //     if let Some(kv) = resp.kvs().first() {
    //         let info: NodeInfo = serde_yaml::from_slice(kv.value())?;
    //         Ok(Some(info))
    //     } else {
    //         Ok(None)
    //     }
    // }

    pub async fn list_nodes(&self) -> Result<Vec<(String, NodeInfo)>> {
        let mut client = self.client.write().await;
        let resp = client
            .get("/registry/nodes/", Some(GetOptions::new().with_prefix()))
            .await?;
        let mut result = Vec::new();
        for kv in resp.kvs() {
            let name = String::from_utf8_lossy(kv.key()).replace("/registry/nodes/", "");
            let info: NodeInfo = serde_yaml::from_slice(kv.value())?;
            result.push((name, info));
        }
        Ok(result)
    }

    pub async fn delete_pod(&self, pod_name: &str) -> Result<()> {
        let key = format!("/registry/pods/{}", pod_name);
        let mut client = self.client.write().await;
        client.delete(key, None).await?;
        Ok(())
    }
    // pub async fn delete_node(&self, node_name: &str) -> Result<()> {
    //     let key = format!("/registry/nodes/{}", node_name);
    //     let mut client = self.client.write().await;
    //     client.delete(key, None).await?;
    //     Ok(())
    // }
}
