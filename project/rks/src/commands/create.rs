use crate::api::xlinestore::XlineStore;
use crate::protocol::{PodTask, RksMessage};
use anyhow::Result;
use quinn::Connection;
use std::sync::Arc;

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
    pod_task: Box<PodTask>,
    xline_store: &Arc<XlineStore>,
    conn: &Connection,
    
) -> Result<()> {
    let node_name = pod_task.nodename.clone();
    let pod_name = pod_task.metadata.name.clone();
    let yaml = serde_yaml::to_string(&*pod_task)?;
    xline_store.insert_pod_yaml(&pod_name, &yaml).await?;

    println!(
        "[user_create] created pod {} on node {} (written to xline)",
        pod_name, node_name
    );

    // ACK
    let response = RksMessage::Ack;
    let data = bincode::serialize(&response)?;
    if let Ok(mut stream) = conn.open_uni().await {
        stream.write_all(&data).await?;
        stream.finish()?;
    }
    Ok(())
}
