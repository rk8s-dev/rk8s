#![allow(unused)]
use crate::api::xlinestore::XlineStore;
use anyhow::Result;
use clap::builder::Str;
use common::{PodTask, RksMessage};
use log::{error, info};
use quinn::Connection;
use std::sync::Arc;

/// Send a pod creation message to a specific worker node
pub async fn watch_create(pod_yaml: String, conn: &Connection, node_id: &str) -> Result<()> {
    if let Ok(pod_task) = serde_yaml::from_str::<PodTask>(&pod_yaml)
        && pod_task.spec.node_name.as_deref() == Some(node_id)
    {
        info!(
            "[watch_pods] PUT matched node={} pod_name={:?}",
            node_id, pod_task.metadata.name
        );

        let msg = RksMessage::CreatePod(Box::new(pod_task));
        let data = bincode::serialize(&msg)?;
        if let Ok(mut stream) = conn.open_uni().await {
            stream.write_all(&data).await?;
            stream.finish()?;
        }
    }
    Ok(())
}

/// Handle user-requested pod creation, store pod in Xline
pub async fn user_create(
    pod_task: Box<PodTask>,
    xline_store: &Arc<XlineStore>,
    conn: &Connection,
) -> Result<()> {
    if (xline_store.get_pod_yaml(&pod_task.metadata.name).await?).is_some() {
        error!(
            "[user_create] Pod {} already exists, creation skipped",
            pod_task.metadata.name
        );

        let response = RksMessage::Error(format!("Pod {} already exists", pod_task.metadata.name));
        let data = bincode::serialize(&response)?;
        if let Ok(mut stream) = conn.open_uni().await {
            stream.write_all(&data).await?;
            stream.finish()?;
        }
        return Ok(());
    }

    // Serialize pod to YAML
    let pod_yaml = match serde_yaml::to_string(&pod_task) {
        Ok(yaml) => yaml,
        Err(e) => {
            eprintln!("[user_create] Failed to serialize pod task: {e}");
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

    info!(
        "[user_create] created pod {} (written to Xline)",
        pod_task.metadata.name
    );

    // Send ACK to user
    let response = RksMessage::Ack;
    let data = bincode::serialize(&response)?;
    if let Ok(mut stream) = conn.open_uni().await {
        stream.write_all(&data).await?;
        stream.finish()?;
    }

    Ok(())
}
