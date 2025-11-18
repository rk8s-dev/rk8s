use crate::api::xlinestore::XlineStore;
use crate::commands::{create, delete};
use chrono::Utc;
use common::quic::RksConnection;
use common::{Node, NodeStatus, PodTask, RksMessage};
use log::{error, info, warn};
use std::sync::Arc;

/// Dispatch worker-originated messages
pub async fn dispatch_worker(
    msg: RksMessage,
    conn: &RksConnection,
    xline_store: &Arc<XlineStore>,
) -> anyhow::Result<()> {
    match msg {
        RksMessage::Heartbeat { node_name, status } => {
            handle_heartbeat(xline_store, &node_name, status).await?;
            conn.send_msg(&RksMessage::Ack).await?;
        }
        RksMessage::Error(err_msg) => error!(
            target: "rks::node::worker_dispatch",
            "reported error: {err_msg}"
        ),
        RksMessage::Ack => info!(
            target: "rks::node::worker_dispatch",
            "received Ack"
        ),

        RksMessage::SetPodip((pod_name, pod_ip)) => {
            if let Some(pod_yaml) = xline_store.get_pod_yaml(&pod_name).await? {
                let mut pod: PodTask = serde_yaml::from_str(&pod_yaml)?;
                pod.status.pod_ip = Some(pod_ip.clone());
                let new_yaml = serde_yaml::to_string(&pod)?;
                xline_store.insert_pod_yaml(&pod_name, &new_yaml).await?;
                info!(
                    target: "rks::node::worker_dispatch",
                    "updated Pod {pod_name} with IP {pod_ip}"
                );
            } else {
                warn!(
                    target: "rks::node::worker_dispatch",
                    "Pod {pod_name} not found when setting IP"
                );
            }
        }
        _ => warn!(
            target: "rks::node::worker_dispatch",
            "unknown or unexpected message from worker"
        ),
    }
    Ok(())
}

/// Handle user-originated messages
pub async fn dispatch_user(
    msg: RksMessage,
    conn: &RksConnection,
    xline_store: &Arc<XlineStore>,
) -> anyhow::Result<()> {
    match msg {
        RksMessage::CreatePod(pod_task) => {
            create::user_create(pod_task, xline_store, conn).await?;
        }
        RksMessage::DeletePod(pod_name) => {
            delete::user_delete(pod_name, xline_store, conn).await?;
        }

        RksMessage::ListPod => {
            let pods = xline_store.list_pod_names().await?;
            info!(
                target: "rks::node::user_dispatch",
                "list current pod: {pods:?}"
            );
            conn.send_msg(&RksMessage::ListPodRes(pods)).await?;
        }
        RksMessage::CreateReplicaSet(mut rs) => {
            let name = rs.metadata.name.clone();
            // Set creation_timestamp if not already set
            if rs.metadata.creation_timestamp.is_none() {
                rs.metadata.creation_timestamp = Some(Utc::now());
            }
            let yaml = serde_yaml::to_string(&*rs)?;
            xline_store.insert_replicaset_yaml(&name, &yaml).await?;
            info!(
                target: "rks::node::user_dispatch",
                "created ReplicaSet {name}"
            );
            conn.send_msg(&RksMessage::Ack).await?;
        }

        RksMessage::UpdateReplicaSet(rs) => {
            let name = rs.metadata.name.clone();
            let yaml = serde_yaml::to_string(&*rs)?;
            xline_store.insert_replicaset_yaml(&name, &yaml).await?;
            info!(
                target: "rks::node::user_dispatch",
                "updated ReplicaSet {name}"
            );
            conn.send_msg(&RksMessage::Ack).await?;
        }

        RksMessage::DeleteReplicaSet(name) => {
            // now just use delete_object with Background policy
            xline_store
                .delete_object(
                    common::ResourceKind::ReplicaSet,
                    &name,
                    common::DeletePropagationPolicy::Background,
                )
                .await?;
            info!(
                target: "rks::node::user_dispatch",
                "marked ReplicaSet {} for deletion (background policy)",
                name
            );
            conn.send_msg(&RksMessage::Ack).await?;
        }

        RksMessage::GetReplicaSet(name) => {
            if let Some(yaml) = xline_store.get_replicaset_yaml(&name).await? {
                let rs: common::ReplicaSet = serde_yaml::from_str(&yaml)?;
                info!(
                    target: "rks::node::user_dispatch",
                    "retrieved ReplicaSet {name}"
                );
                conn.send_msg(&RksMessage::GetReplicaSetRes(Box::new(rs)))
                    .await?;
            } else {
                conn.send_msg(&RksMessage::Error(format!("ReplicaSet {} not found", name)))
                    .await?;
            }
        }

        RksMessage::ListReplicaSet => {
            let rss = xline_store.list_replicasets().await?;
            info!(
                target: "rks::node::user_dispatch",
                "list current replicasets: {} items",
                rss.len()
            );
            conn.send_msg(&RksMessage::ListReplicaSetRes(rss)).await?;
        }

        RksMessage::GetNodeCount => {
            info!(
                target: "rks::node::user_dispatch",
                "GetNodeCount received"
            );
        }
        _ => warn!(
            target: "rks::node::user_dispatch",
            "unknown message"
        ),
    }
    Ok(())
}

async fn handle_heartbeat(
    xline_store: &Arc<XlineStore>,
    node_name: &str,
    status: NodeStatus,
) -> anyhow::Result<()> {
    if let Some(node_yaml) = xline_store.get_node_yaml(node_name).await? {
        let mut node: Node = serde_yaml::from_str(&node_yaml)?;
        node.status = status;

        // Use rks clock as heartbeat time.
        node.set_last_heartbeat_time(Utc::now());
        node.spec.taints = Node::derive_taints_from_conditions(&node.status.conditions);

        let new_yaml = serde_yaml::to_string(&node)?;
        xline_store.insert_node_yaml(node_name, &new_yaml).await?;
        info!(
            target: "rks::node::worker_dispatch",
            "heartbeat updated Node {node_name}"
        );
        return Ok(());
    }

    warn!("heartbeat received for unknown node: {node_name}");
    Ok(())
}
