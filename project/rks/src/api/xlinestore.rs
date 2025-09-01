//use crate::protocol::Node;
use anyhow::Result;
use etcd_client::{Client, GetOptions, PutOptions,WatchOptions,WatchStream,Watcher};
//use serde_yaml;
use std::sync::Arc;
use tokio::sync::RwLock;
/// like etcd, k:/registry/pods/pod_name v:yaml file of pod
/// k:/registry/nodes/node_name v:yaml file of node
#[derive(Clone)]
pub struct XlineStore {
    client: Arc<RwLock<Client>>,
}

impl XlineStore {
    pub async fn new(endpoints: &[&str]) -> Result<Self> {
        let client = Client::connect(endpoints, None).await?;
        Ok(Self {
            client: Arc::new(RwLock::new(client)),
        })
    }

    /// 获取内部的 client 引用（用于 watch 操作）
    pub async fn client(&self) -> tokio::sync::RwLockReadGuard<'_, Client> {
        self.client.read().await
    }

    pub async fn list_pods(&self) -> Result<Vec<String>> {
        let key = "/registry/pods/".to_string();
        let mut client = self.client.write().await;
        let resp = client
            .get(
                key.clone(),
                Some(GetOptions::new().with_prefix().with_keys_only()),
            )
            .await?;
        Ok(resp
            .kvs()
            .iter()
            .map(|kv| String::from_utf8_lossy(kv.key()).replace("/registry/pods/", ""))
            .collect())
    }

    pub async fn insert_node_yaml(&self, node_name: &str, node_yaml: &str) -> Result<()> {
        let key = format!("/registry/nodes/{node_name}");
        let mut client = self.client.write().await;
        client.put(key, node_yaml, Some(PutOptions::new())).await?;
        Ok(())
    }

    // pub async fn get_node_yaml(&self, node_name: &str) -> Result<Option<String>> {
    //     let key = format!("/registry/nodes/{node_name}");
    //     let mut client = self.client.write().await;
    //     let resp = client.get(key, None).await?;
    //     Ok(resp.kvs().first().map(|kv| String::from_utf8_lossy(kv.value()).to_string()))
    // }

    // pub async fn get_node(&self, node_name: &str) -> Result<Option<Node>> {
    //     if let Some(yaml) = self.get_node_yaml(node_name).await? {
    //         let node: Node = serde_yaml::from_str(&yaml)?;
    //         Ok(Some(node))
    //     } else {
    //         Ok(None)
    //     }
    // }

    pub async fn insert_pod_yaml(&self, pod_name: &str, pod_yaml: &str) -> Result<()> {
        let key = format!("/registry/pods/{pod_name}");
        let mut client = self.client.write().await;
        client.put(key, pod_yaml, Some(PutOptions::new())).await?;
        Ok(())
    }

    pub async fn get_pod_yaml(&self, pod_name: &str) -> Result<Option<String>> {
        let key = format!("/registry/pods/{pod_name}");
        let mut client = self.client.write().await;
        let resp = client.get(key, None).await?;
        if let Some(kv) = resp.kvs().first() {
            Ok(Some(String::from_utf8_lossy(kv.value()).to_string()))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_pod(&self, pod_name: &str) -> Result<()> {
        let key = format!("/registry/pods/{pod_name}");
        let mut client = self.client.write().await;
        client.delete(key, None).await?;
        Ok(())
    }

    /// 取得 `/registry/pods/` 的快照以及该次读取到的 revision。
    pub async fn pods_snapshot_with_rev(&self) -> Result<(Vec<(String, String)>, i64)> {
        let key_prefix = "/registry/pods/".to_string();
        let mut client = self.client.write().await;
        // 需要值用于反序列化
        let resp = client
            .get(
                key_prefix.clone(),
                Some(GetOptions::new().with_prefix()),
            )
            .await?;
        let rev = resp.header().map(|h| h.revision()).unwrap_or(0);
        let items: Vec<(String, String)> = resp
            .kvs()
            .iter()
            .map(|kv| {
                (
                    String::from_utf8_lossy(kv.key()).replace("/registry/pods/", ""),
                    String::from_utf8_lossy(kv.value()).to_string(),
                )
            })
            .collect();
        Ok((items, rev))
    }

    /// 建立 pods 的前缀 watch（带 prev_kv，起始 revision）。
    pub async fn watch_pods(&self, start_rev: i64) -> Result<(Watcher, WatchStream)> {
        let key_prefix = "/registry/pods/".to_string();
        let opts = WatchOptions::new()
            .with_prefix()
            .with_prev_key()
            .with_start_revision(start_rev);
        let mut client = self.client.write().await;
        let (watcher, stream) = client.watch(key_prefix, Some(opts)).await?;
        Ok((watcher, stream))
    }

    pub async fn init_flannel_config(&self) -> Result<()> {
        let config_json = r#"{
            "Network": "10.244.0.0/16",
            "SubnetLen": 24,
            "Backend": {
                "Type": "vxlan",
                "VNI": 1
            }
        }"#;

        let key = "/coreos.com/network/config";
        let mut client = self.client.write().await;
        client.put(key, config_json, Some(PutOptions::new())).await?;
        Ok(())
    }
    
    // pub async fn delete_node(&self, node_name: &str) -> Result<()> {
    //     let key = format!("/registry/nodes/{}", node_name);
    //     let mut client = self.client.write().await;
    //     client.delete(key, None).await?;
    //     Ok(())
    // }
}
