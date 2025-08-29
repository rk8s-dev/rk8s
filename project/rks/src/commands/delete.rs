use crate::api::xlinestore::XlineStore;
use anyhow::Result;
use common::{PodTask, RksMessage};
use quinn::Connection;
use std::sync::Arc;
use tokio::sync::broadcast;

pub async fn watch_delete(
    pod_name: String,
    conn: &Connection,
    xline_store: &Arc<XlineStore>,
    node_id: &str,
) -> Result<()> {
    let msg = RksMessage::DeletePod(pod_name.clone());
    if let Ok(pods) = xline_store.list_pods().await {
        for p in pods {
            if let Ok(Some(pod_yaml)) = xline_store.get_pod_yaml(&p).await {
                let pod_task: PodTask = serde_yaml::from_str(&pod_yaml)
                    .map_err(|e| anyhow::anyhow!("Failed to parse pod_yaml: {}", e))?;
                if pod_task.spec.nodename.as_deref() == Some(node_id)
                    && pod_task.metadata.name == pod_name
                {
                    let data = bincode::serialize(&msg)?;
                    if let Ok(mut stream) = conn.open_uni().await {
                        stream.write_all(&data).await?;
                        stream.finish()?;
                    }
                    let _ = xline_store.delete_pod(&pod_name).await;
                    println!("[user dispatch] deleted pod: {pod_name}");
                    break;
                }
            }
        }
    }
    Ok(())
}

pub async fn user_delete(
    pod_name: String,
    _xline_store: &Arc<XlineStore>,
    conn: &Connection,
    tx: &broadcast::Sender<RksMessage>,
) -> Result<()> {
    let _ = tx.send(RksMessage::DeletePod(pod_name.clone()));

    let response = RksMessage::Ack;
    let data = bincode::serialize(&response)?;
    if let Ok(mut stream) = conn.open_uni().await {
        stream.write_all(&data).await?;
        stream.finish()?;
    }
    Ok(())
}
