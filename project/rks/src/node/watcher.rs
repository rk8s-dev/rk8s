use crate::commands::create::watch_create;
use crate::commands::delete::watch_delete;
use crate::node::Shared;
use common::quic::RksConnection;
use common::{PodTask, RksMessage, log_error};
use etcd_client::{KeyValue, WatchResponse};
use log::{error, info};
use std::ops::Deref;
use std::sync::Arc;
use tonic::codegen::tokio_stream::StreamExt;

/// Watches pod changes from Xline and pushes create/delete events to the worker node.
#[derive(Clone)]
pub struct PodsWatcher {
    node_id: String,
    conn: RksConnection,
    shared: Arc<Shared>,
}

impl PodsWatcher {
    pub fn new(node_id: impl Into<String>, conn: RksConnection, shared: Arc<Shared>) -> Self {
        Self {
            node_id: node_id.into(),
            conn,
            shared,
        }
    }

    pub fn spawn(&self) -> anyhow::Result<()> {
        let watcher = self.clone();
        tokio::spawn(async move {
            log_error!(watcher.run().await);
        });

        self.spawn_lease_completion();
        Ok(())
    }

    async fn run(self) -> anyhow::Result<()> {
        let (node_id, start_rev) = self.send_initial_snapshot().await?;
        self.stream_updates(node_id, start_rev + 1).await
    }

    async fn handle_watch_event(
        &self,
        resp: WatchResponse,
        node_id: impl AsRef<str>,
    ) -> anyhow::Result<()> {
        let node_id = node_id.as_ref();

        for event in resp.events() {
            match event.event_type() {
                etcd_client::EventType::Put => {
                    if let Some(kv) = event.kv() {
                        self.handle_put_event(node_id, kv, event.prev_kv()).await?;
                    }
                }
                etcd_client::EventType::Delete => {
                    if let Some(kv) = event.prev_kv() {
                        watch_delete(
                            String::from_utf8_lossy(kv.key()).replace("/registry/pods/", ""),
                            String::from_utf8_lossy(kv.value()).to_string(),
                            self.conn.deref(),
                            node_id,
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn send_initial_snapshot(&self) -> anyhow::Result<(String, i64)> {
        let node_id = self.node_id.clone();
        // Get current snapshot and revision
        let (pods, rev) = self.shared.xline_store.pods_snapshot_with_rev().await?;

        for (pod_name, pod_yaml) in pods {
            let pod_task = serde_yaml::from_str::<PodTask>(&pod_yaml)?;
            // Send snapshot to the worker
            if pod_task.spec.node_name.as_deref() == Some(node_id.as_str()) {
                self.conn
                    .send_msg(&RksMessage::CreatePod(Box::new(pod_task)))
                    .await?;
                info!(
                    target: "rks::node::watch_pods",
                    "sent existing pod to worker: {pod_name}"
                );
            }
        }

        Ok((node_id, rev))
    }

    async fn stream_updates(&self, node_id: String, start_rev: i64) -> anyhow::Result<()> {
        // Start watching for changes
        let (mut watcher, mut stream) = self.shared.xline_store.watch_pods(start_rev).await?;
        info!(
            target: "rks::node::watch_pods",
            "start watching pods from revision {start_rev}"
        );

        while let Some(resp) = stream.next().await {
            match resp {
                Ok(resp) => self.handle_watch_event(resp, &node_id).await?,
                Err(e) => {
                    error!(
                        target: "rks::node::watch_pods",
                        "watch stream error: {e}"
                    );
                    break;
                }
            }
        }

        watcher.cancel().await?;
        Ok(())
    }

    fn spawn_lease_completion(&self) {
        let node_id = self.node_id.clone();
        let node_registry = self.shared.node_registry.clone();
        let local_manager = self.shared.local_manager.clone();
        tokio::spawn(async move {
            if let Some(worker_session) = node_registry.get(&node_id).await {
                let lease = worker_session.lease.clone();
                let cancel = worker_session.cancel_notify.clone();

                if let Err(e) = local_manager.complete_lease(lease, cancel).await {
                    error!("complete_lease error for node={node_id}: {e:?}");
                }
                return;
            }

            error!("no active worker session for node={node_id}");
        });
    }

    async fn handle_put_event(
        &self,
        node_id: &str,
        kv: &KeyValue,
        prev_kv: Option<&KeyValue>,
    ) -> anyhow::Result<()> {
        let new_pod: PodTask = serde_yaml::from_slice(kv.value())?;

        if let Some(prev_kv) = prev_kv {
            let prev_pod: PodTask = serde_yaml::from_slice(prev_kv.value())?;

            // Only updating node_name can be watched and send to node
            if prev_pod.spec.node_name.is_none() && new_pod.spec.node_name.is_some() {
                self.enqueue_create(node_id, kv.value(), &new_pod).await?;
            }

            return Ok(());
        }

        // If the nodename is assigned at first, be watched by node
        self.enqueue_create(node_id, kv.value(), &new_pod).await
    }

    async fn enqueue_create(
        &self,
        node_id: &str,
        payload: &[u8],
        pod: &PodTask,
    ) -> anyhow::Result<()> {
        info!(
            target: "rks::node::watch_pods",
            "Pod {} assigned to {:?}",
            pod.metadata.name, pod.spec.node_name
        );

        watch_create(
            String::from_utf8_lossy(payload).to_string(),
            self.conn.deref(),
            node_id,
        )
        .await
    }
}
