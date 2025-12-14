#![allow(unused)]
use crate::api::xlinestore::XlineStore;
use anyhow::Result;
use clap::builder::Str;
use common::quic::SendStreamExt;
use common::{PodTask, RksMessage};
use log::info;
use quinn::Connection;
use std::sync::Arc;
pub async fn watch_delete(
    pod_name: String,
    pod_yaml: String,
    conn: &Connection,
    node_id: &str,
) -> Result<()> {
    if let Ok(pod_task) = serde_yaml::from_str::<PodTask>(&pod_yaml)
        && pod_task.spec.node_name.as_deref() == Some(node_id)
    {
        info!(
            target: "rks::node::watch_pods",
            "DELETE pod_name={pod_name} for node={node_id}"
        );

        let msg = RksMessage::DeletePod(pod_name.clone());
        if let Ok(mut stream) = conn.open_uni().await {
            stream.send_msg(&msg).await?;
            info!(
                target: "rks::node::watch_pods",
                "sent delete pod to worker {node_id}"
            );
        }
    }
    Ok(())
}

pub async fn user_delete(
    pod_name: String,
    xline_store: &Arc<XlineStore>,
    conn: &Connection,
) -> Result<()> {
    xline_store.delete_pod(&pod_name).await?;
    info!(
        target: "rks::commands::user_delete",
        "deleted pod {pod_name} (written to xline)"
    );

    let response = RksMessage::Ack;
    if let Ok(mut stream) = conn.open_uni().await {
        stream.send_msg(&response).await?;
    }
    Ok(())
}
