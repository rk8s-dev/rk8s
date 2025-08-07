use crate::api::xlinestore::XlineStore;
use crate::protocol::{PodTask, RksMessage};
use anyhow::Result;
use quinn::Connection;
use std::sync::Arc;
use tokio::sync::broadcast;

pub async fn watch_create(pod_task: &PodTask, conn: &Connection, node_id: &str) -> Result<()> {
    if pod_task.nodename == node_id {
        let msg = RksMessage::CreatePod(Box::new(pod_task.clone()));
        let data = bincode::serialize(&msg)?;
        if let Ok(mut stream) = conn.open_uni().await {
            stream.write_all(&data).await?;
            stream.finish()?;
            println!(
                "[watch_pods] send CreatePod for pod: {} to node: {}",
                pod_task.metadata.name, node_id
            );
        }
    }
    Ok(())
}

pub async fn user_create(
    mut pod_task: Box<PodTask>,
    xline_store: &Arc<XlineStore>,
    conn: &Connection,
    tx: &broadcast::Sender<RksMessage>,
) -> Result<()> {
    if let Ok(nodes) = xline_store.list_nodes().await {
        if let Some((node_name, _)) = nodes.first() {
            pod_task.nodename = node_name.clone();
            let pod_yaml = match serde_yaml::to_string(&pod_task) {
                Ok(yaml) => yaml,
                Err(e) => {
                    eprintln!("[user dispatch] Failed to serialize pod task: {e}");
                    let response = RksMessage::Error(format!("Serialization error: {e}"));
                    let data = bincode::serialize(&response).unwrap_or_else(|_| vec![]);
                    if let Ok(mut stream) = conn.open_uni().await {
                        stream.write_all(&data).await?;
                        stream.finish()?;
                    }
                    return Ok(());
                }
            };

            xline_store
                .insert_pod_yaml(&pod_task.metadata.name, &pod_yaml)
                .await?;

            println!(
                "[user dispatch] created pod: {}, assigned to node: {}",
                pod_task.metadata.name, node_name
            );

            let _ = tx.send(RksMessage::CreatePod(pod_task.clone()));

            let response = RksMessage::Ack;
            let data = bincode::serialize(&response)?;
            if let Ok(mut stream) = conn.open_uni().await {
                stream.write_all(&data).await?;
                stream.finish()?;
            }
        }
    }
    Ok(())
}
