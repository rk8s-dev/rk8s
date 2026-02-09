#![allow(unused)]
use crate::api::xlinestore::XlineStore;
use anyhow::Result;
use chrono::Utc;
use clap::builder::Str;
use common::quic::SendStreamExt;
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
            target: "rks::node::watch_pods",
            "PUT matched node={node_id} pod_name={:?}",
            pod_task.metadata.name
        );

        let msg = RksMessage::CreatePod(Box::new(pod_task));
        if let Ok(mut stream) = conn.open_uni().await {
            stream.send_msg(&msg).await?;
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
            target: "rks::commands::user_create",
            "Pod {} already exists, creation skipped",
            pod_task.metadata.name
        );

        let response = RksMessage::Error(format!("Pod {} already exists", pod_task.metadata.name));
        if let Ok(mut stream) = conn.open_uni().await {
            stream.send_msg(&response).await?;
        }
        return Ok(());
    }

    let mut pod_task = pod_task;
    pod_task.metadata.creation_timestamp = Some(Utc::now());

    // Serialize pod to YAML
    let pod_yaml = match serde_yaml::to_string(&pod_task) {
        Ok(yaml) => yaml,
        Err(e) => {
            error!(
                target: "rks::commands::user_create",
                "failed to serialize pod task: {e}"
            );
            let response = RksMessage::Error(format!("Serialization error: {e}"));
            if let Ok(mut stream) = conn.open_uni().await {
                let _ = stream.send_msg(&response).await;
            }
            return Ok(());
        }
    };

    xline_store
        .insert_pod_yaml(&pod_task.metadata.name, &pod_yaml)
        .await?;

    info!(
        target: "rks::commands::user_create",
        "created pod {} (written to Xline)",
        pod_task.metadata.name
    );

    // Send ACK to user
    let response = RksMessage::Ack;
    if let Ok(mut stream) = conn.open_uni().await {
        stream.send_msg(&response).await?;
    }

    Ok(())
}
