use crate::protocol::config::NetworkConfig;
use anyhow::Result;
use common::*;
use etcd_client::{Client, GetOptions, PutOptions, WatchOptions, WatchStream, Watcher};
use libvault::storage::xline::XlineOptions;
use log::error;
use std::sync::Arc;
use tokio::sync::RwLock;

/// XlineStore provides an etcd-like API for managing pods and nodes.
/// Keys are stored under `/registry/pods/` and `/registry/nodes/`.
/// Values are YAML serialized definitions.
#[derive(Clone)]
pub struct XlineStore {
    client: Arc<RwLock<Client>>,
}

#[allow(unused)]
impl XlineStore {
    /// Create a new XlineStore instance by connecting to the given endpoints.
    pub async fn new(option: XlineOptions) -> Result<Self> {
        let client = Client::connect(option.endpoints, option.config).await?;
        Ok(Self {
            client: Arc::new(RwLock::new(client)),
        })
    }

    /// Get a read-only reference to the internal etcd client.
    /// This is typically used for watch operations.
    pub async fn client(&self) -> tokio::sync::RwLockReadGuard<'_, Client> {
        self.client.read().await
    }

    /// List all pod names (keys only, values are ignored).
    pub async fn list_pod_names(&self) -> Result<Vec<String>> {
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

    /// List all node names (keys only, values are ignored).
    pub async fn list_node_names(&self) -> Result<Vec<String>> {
        let key = "/registry/nodes/".to_string();
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
            .map(|kv| String::from_utf8_lossy(kv.key()).replace("/registry/nodes/", ""))
            .collect())
    }

    pub async fn list_nodes(&self) -> Result<Vec<Node>> {
        let key = "/registry/nodes/".to_string();
        let mut client = self.client.write().await;
        let resp = client
            .get(key.clone(), Some(GetOptions::new().with_prefix()))
            .await?;

        let nodes: Vec<Node> = resp
            .kvs()
            .iter()
            .filter_map(|kv| {
                let yaml_str = String::from_utf8_lossy(kv.value());
                serde_yaml::from_str::<Node>(&yaml_str).ok()
            })
            .collect();

        Ok(nodes)
    }

    pub async fn list_pods(&self) -> Result<Vec<PodTask>> {
        let key = "/registry/pods/".to_string();
        let mut client = self.client.write().await;
        let resp = client
            .get(key.clone(), Some(GetOptions::new().with_prefix()))
            .await?;

        let pods: Vec<PodTask> = resp
            .kvs()
            .iter()
            .filter_map(|kv| {
                let yaml_str = String::from_utf8_lossy(kv.value());
                serde_yaml::from_str::<PodTask>(&yaml_str).ok()
            })
            .collect();

        Ok(pods)
    }

    /// Insert a node YAML definition into xline.
    pub async fn insert_node_yaml(&self, node_name: &str, node_yaml: &str) -> Result<()> {
        let key = format!("/registry/nodes/{node_name}");
        let mut client = self.client.write().await;
        client.put(key, node_yaml, Some(PutOptions::new())).await?;
        Ok(())
    }

    pub async fn insert_node(&self, node: &Node) -> Result<()> {
        let node_name = node.metadata.name.clone();
        if node_name.is_empty() {
            anyhow::bail!("node.metadata.name is empty");
        }

        let node_yaml = serde_yaml::to_string(node)?;
        self.insert_node_yaml(&node_name, &node_yaml).await
    }

    // Example (currently unused):
    pub async fn get_node_yaml(&self, node_name: &str) -> Result<Option<String>> {
        let key = format!("/registry/nodes/{node_name}");
        let mut client = self.client.write().await;
        let resp = client.get(key, None).await?;
        Ok(resp
            .kvs()
            .first()
            .map(|kv| String::from_utf8_lossy(kv.value()).to_string()))
    }

    pub async fn get_node(&self, node_name: &str) -> Result<Option<Node>> {
        if let Some(yaml) = self.get_node_yaml(node_name).await? {
            let node: Node = serde_yaml::from_str(&yaml)?;
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    /// Insert a pod YAML definition into xline.
    pub async fn insert_pod_yaml(&self, pod_name: &str, pod_yaml: &str) -> Result<()> {
        let key = format!("/registry/pods/{pod_name}");
        let mut client = self.client.write().await;
        client.put(key, pod_yaml, Some(PutOptions::new())).await?;
        Ok(())
    }

    /// Get a pod YAML definition from xline.
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

    pub async fn get_pod(&self, pod_name: &str) -> Result<Option<PodTask>> {
        match self.get_pod_yaml(pod_name).await? {
            Some(yaml) => Ok(Some(serde_yaml::from_str::<PodTask>(&yaml)?)),
            None => Ok(None),
        }
    }

    /// Delete a pod from xline.
    pub async fn delete_pod(&self, pod_name: &str) -> Result<()> {
        let key = format!("/registry/pods/{pod_name}");
        let mut client = self.client.write().await;
        client.delete(key, None).await?;
        Ok(())
    }

    pub async fn delete_node(&self, node_name: &str) -> Result<()> {
        let key = format!("/registry/nodes/{node_name}");
        let mut client = self.client.write().await;
        client.delete(key, None).await?;
        Ok(())
    }

    pub async fn insert_network_config(&self, prefix: &str, config: &NetworkConfig) -> Result<()> {
        let key = format!("{}/config", prefix.trim_end_matches('/'));

        let value = serde_json::to_string(config)?;

        let mut client = self.client.write().await;
        client.put(key, value, Some(PutOptions::new())).await?;
        Ok(())
    }

    pub async fn get_network_config(&self, prefix: &str) -> Result<Option<NetworkConfig>> {
        let key = format!("{}/config", prefix.trim_end_matches('/'));
        let mut client = self.client.write().await;
        let resp = client.get(key, None).await?;
        if let Some(kv) = resp.kvs().first() {
            let cfg: NetworkConfig = serde_json::from_slice(kv.value())?;
            Ok(Some(cfg))
        } else {
            Ok(None)
        }
    }

    /// Take a snapshot of all pods and return them with the current revision.
    pub async fn pods_snapshot_with_rev(&self) -> Result<(Vec<(String, String)>, i64)> {
        let key_prefix = "/registry/pods/".to_string();
        let mut client = self.client.write().await;
        let resp = client
            .get(key_prefix.clone(), Some(GetOptions::new().with_prefix()))
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

    /// Create a watch on all pods with prefix `/registry/pods/`, starting from a given revision.
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

    /// Initialize Flannel CNI network configuration.
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
        client
            .put(key, config_json, Some(PutOptions::new()))
            .await?;
        Ok(())
    }

    /// List all service names (keys only, values are ignored).
    pub async fn list_service_names(&self) -> Result<Vec<String>> {
        let key = "/registry/services/".to_string();
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
            .map(|kv| String::from_utf8_lossy(kv.key()).replace("/registry/services/", ""))
            .collect())
    }

    /// List all services (deserialize values).
    pub async fn list_services(&self) -> Result<Vec<ServiceTask>> {
        let key = "/registry/services/".to_string();
        let mut client = self.client.write().await;
        let resp = client
            .get(key.clone(), Some(GetOptions::new().with_prefix()))
            .await?;

        let services: Vec<ServiceTask> = resp
            .kvs()
            .iter()
            .filter_map(|kv| {
                let yaml_str = String::from_utf8_lossy(kv.value());
                serde_yaml::from_str::<ServiceTask>(&yaml_str).ok()
            })
            .collect();

        Ok(services)
    }

    /// List all endpoints (deserialize values).
    pub async fn list_endpoints(&self) -> Result<Vec<Endpoint>> {
        let key = "/registry/endpoints/".to_string();
        let mut client = self.client.write().await;
        let resp = client
            .get(key.clone(), Some(GetOptions::new().with_prefix()))
            .await?;
        let endpoints: Vec<Endpoint> = resp
            .kvs()
            .iter()
            .filter_map(|kv| {
                let yaml_str = String::from_utf8_lossy(kv.value());
                match serde_yaml::from_str::<Endpoint>(&yaml_str) {
                    Ok(ep) => Some(ep),
                    Err(e) => {
                        error!(
                            "failed to parse Endpoint at key {:?}: {}\nvalue:\n{}",
                            kv.key(),
                            e,
                            yaml_str
                        );
                        None
                    }
                }
            })
            .collect();
        Ok(endpoints)
    }

    /// Insert a service YAML definition into xline.
    pub async fn insert_service_yaml(&self, service_name: &str, service_yaml: &str) -> Result<()> {
        let key = format!("/registry/services/{service_name}");
        let mut client = self.client.write().await;
        client
            .put(key, service_yaml, Some(PutOptions::new()))
            .await?;
        Ok(())
    }

    /// Insert an endpoints YAML definition into xline.
    pub async fn insert_endpoint_yaml(
        &self,
        endpoint_name: &str,
        endpoint_yaml: &str,
    ) -> Result<()> {
        let key = format!("/registry/endpoints/{endpoint_name}");
        let mut client = self.client.write().await;
        client
            .put(key, endpoint_yaml, Some(PutOptions::new()))
            .await?;
        Ok(())
    }

    /// Get a service YAML definition from xline.
    pub async fn get_service_yaml(&self, service_name: &str) -> Result<Option<String>> {
        let key = format!("/registry/services/{service_name}");
        let mut client = self.client.write().await;
        let resp = client.get(key, None).await?;
        Ok(resp
            .kvs()
            .first()
            .map(|kv| String::from_utf8_lossy(kv.value()).to_string()))
    }

    /// Get a service object from xline.
    pub async fn get_service(&self, service_name: &str) -> Result<Option<ServiceTask>> {
        if let Some(yaml) = self.get_service_yaml(service_name).await? {
            let service: ServiceTask = serde_yaml::from_str(&yaml)?;
            Ok(Some(service))
        } else {
            Ok(None)
        }
    }

    /// Delete a service from xline.
    pub async fn delete_service(&self, service_name: &str) -> Result<()> {
        let key = format!("/registry/services/{service_name}");
        let mut client = self.client.write().await;
        client.delete(key, None).await?;
        Ok(())
    }

    /// Create a watch on all pods with prefix `/registry/services/`, starting from a given revision.
    pub async fn watch_services(&self, start_rev: i64) -> Result<(Watcher, WatchStream)> {
        let key_prefix = "/registry/services/".to_string();
        let opts = WatchOptions::new()
            .with_prefix()
            .with_prev_key()
            .with_start_revision(start_rev);
        let mut client = self.client.write().await;
        let (watcher, stream) = client.watch(key_prefix, Some(opts)).await?;
        Ok((watcher, stream))
    }

    /// Create a watch on all endpoints with prefix `/registry/endpoints/`, starting from a given revision.
    pub async fn watch_endpoints(&self, start_rev: i64) -> Result<(Watcher, WatchStream)> {
        let key_prefix = "/registry/endpoints/".to_string();
        let opts = WatchOptions::new()
            .with_prefix()
            .with_prev_key()
            .with_start_revision(start_rev);
        let mut client = self.client.write().await;
        let (watcher, stream) = client.watch(key_prefix, Some(opts)).await?;
        Ok((watcher, stream))
    }

    /// Insert a replicaset YAML definition into xline.
    pub async fn insert_replicaset_yaml(&self, rs_name: &str, rs_yaml: &str) -> Result<()> {
        let key = format!("/registry/replicasets/{rs_name}");
        let mut client = self.client.write().await;
        client.put(key, rs_yaml, Some(PutOptions::new())).await?;
        Ok(())
    }

    /// Get a replicaset YAML definition from xline.
    pub async fn get_replicaset_yaml(&self, rs_name: &str) -> Result<Option<String>> {
        let key = format!("/registry/replicasets/{rs_name}");
        let mut client = self.client.write().await;
        let resp = client.get(key, None).await?;
        Ok(resp
            .kvs()
            .first()
            .map(|kv| String::from_utf8_lossy(kv.value()).to_string()))
    }

    /// Delete a replicaset from xline.
    pub async fn delete_replicaset(&self, rs_name: &str) -> Result<()> {
        let key = format!("/registry/replicasets/{rs_name}");
        let mut client = self.client.write().await;
        client.delete(key, None).await?;
        Ok(())
    }

    /// List all replicaset YAMLs (deserialize values).
    pub async fn list_replicasets(&self) -> Result<Vec<ReplicaSet>> {
        let key = "/registry/replicasets/".to_string();
        let mut client = self.client.write().await;
        let resp = client
            .get(key.clone(), Some(GetOptions::new().with_prefix()))
            .await?;

        let rss: Vec<ReplicaSet> = resp
            .kvs()
            .iter()
            .filter_map(|kv| {
                let yaml_str = String::from_utf8_lossy(kv.value());
                serde_yaml::from_str::<ReplicaSet>(&yaml_str).ok()
            })
            .collect();

        Ok(rss)
    }

    /// Take a snapshot of all replicasets and return them with the current revision.
    pub async fn replicasets_snapshot_with_rev(&self) -> Result<(Vec<(String, String)>, i64)> {
        let key_prefix = "/registry/replicasets/".to_string();
        let mut client = self.client.write().await;
        let resp = client
            .get(key_prefix.clone(), Some(GetOptions::new().with_prefix()))
            .await?;
        let rev = resp.header().map(|h| h.revision()).unwrap_or(0);
        let items: Vec<(String, String)> = resp
            .kvs()
            .iter()
            .map(|kv| {
                (
                    String::from_utf8_lossy(kv.key()).replace("/registry/replicasets/", ""),
                    String::from_utf8_lossy(kv.value()).to_string(),
                )
            })
            .collect();
        Ok((items, rev))
    }

    /// Create a watch on all replicasets with prefix `/registry/replicasets/`, starting from a given revision.
    pub async fn watch_replicasets(&self, start_rev: i64) -> Result<(Watcher, WatchStream)> {
        let key_prefix = "/registry/replicasets/".to_string();
        let opts = WatchOptions::new()
            .with_prefix()
            .with_prev_key()
            .with_start_revision(start_rev);
        let mut client = self.client.write().await;
        let (watcher, stream) = client.watch(key_prefix, Some(opts)).await?;
        Ok((watcher, stream))
    }
}
