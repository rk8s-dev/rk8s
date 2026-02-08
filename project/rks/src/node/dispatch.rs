use crate::api::xlinestore::XlineStore;
use crate::commands::{create, delete};
use chrono::Utc;
use common::quic::RksConnection;
use common::*;
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
        RksMessage::GetPodByUid(pod_uid) => {
            let pods = xline_store.list_pods().await?;
            if let Some(pod) = pods.into_iter().find(|p| p.metadata.uid == pod_uid) {
                info!(
                    target: "rks::node::user_dispatch",
                    "retrieved Pod with UID {pod_uid}"
                );
                conn.send_msg(&RksMessage::GetPodByUidRes(Box::new(pod)))
                    .await?;
            } else {
                conn.send_msg(&RksMessage::Error(format!(
                    "Pod with UID {} not found",
                    pod_uid
                )))
                .await?;
            }
        }
        RksMessage::GetPod(name) => {
            if let Some(pod) = xline_store.get_pod(&name).await? {
                info!(
                    target: "rks::node::user_dispatch",
                    "retrieved Pod {name}"
                );
                conn.send_msg(&RksMessage::GetPodRes(Box::new(pod))).await?;
            } else {
                conn.send_msg(&RksMessage::Error(format!("Pod {} not found", name)))
                    .await?;
            }
        }
        RksMessage::ListPod => {
            let pods = xline_store.list_pods().await?;
            info!(
                target: "rks::node::user_dispatch",
                "list current pods: {} items",
                pods.len()
            );
            conn.send_msg(&RksMessage::ListPodRes(pods)).await?;
        }
        RksMessage::CreateReplicaSet(mut rs) => {
            let name = rs.metadata.name.clone();
            if xline_store
                .get_replicaset_yaml(&rs.metadata.name)
                .await?
                .is_some()
            {
                let err_msg = format!("rs \"{}\" already exists", rs.metadata.name);
                conn.send_msg(&RksMessage::Error(err_msg)).await?;
                return Ok(());
            }
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

        RksMessage::UpdateReplicaSet(incoming_rs) => {
            let name = incoming_rs.metadata.name.clone();
            if let Some(existing_yaml) = xline_store.get_replicaset_yaml(&name).await? {
                let mut final_rs: common::ReplicaSet = serde_yaml::from_str(&existing_yaml)?;
                if final_rs.spec != incoming_rs.spec {
                    let current_gen = final_rs.metadata.generation.unwrap_or(0);
                    final_rs.metadata.generation = Some(current_gen + 1);
                    final_rs.spec = incoming_rs.spec.clone();

                    info!(target: "rks::node::user_dispatch", "ReplicaSet {} spec updated, add generation", name);
                }
                let yaml = serde_yaml::to_string(&final_rs)?;
                xline_store.insert_replicaset_yaml(&name, &yaml).await?;
                info!(target: "rks::node::user_dispatch", "updated ReplicaSet {name} (preserved state)");
            } else {
                let yaml = serde_yaml::to_string(&*incoming_rs)?;
                xline_store.insert_replicaset_yaml(&name, &yaml).await?;
            }
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

        // Deployment operations
        RksMessage::CreateDeployment(mut deploy) => {
            if xline_store
                .get_deployment_yaml(&deploy.metadata.name)
                .await?
                .is_some()
            {
                let err_msg = format!("deployment \"{}\" already exists", deploy.metadata.name);
                conn.send_msg(&RksMessage::Error(err_msg)).await?;
                return Ok(());
            }
            let name = deploy.metadata.name.clone();
            if deploy.metadata.creation_timestamp.is_none() {
                deploy.metadata.creation_timestamp = Some(Utc::now());
            }
            let yaml = serde_yaml::to_string(&*deploy)?;
            xline_store.insert_deployment_yaml(&name, &yaml).await?;
            info!(
                target: "rks::node::user_dispatch",
                "created Deployment {name}"
            );
            conn.send_msg(&RksMessage::Ack).await?;
        }
        RksMessage::UpdateDeployment(incoming_deploy) => {
            let name = incoming_deploy.metadata.name.clone();
            if let Some(existing_yaml) = xline_store.get_deployment_yaml(&name).await? {
                let mut final_deploy: Deployment = serde_yaml::from_str(&existing_yaml)?;
                if final_deploy.spec != incoming_deploy.spec {
                    let current_gen = final_deploy.metadata.generation.unwrap_or(0);
                    final_deploy.metadata.generation = Some(current_gen + 1);
                    final_deploy.spec = incoming_deploy.spec.clone();
                    info!(
                        "Deployment {} spec updated, add generation to {}",
                        name,
                        current_gen + 1
                    );
                }
                let yaml = serde_yaml::to_string(&final_deploy)?;
                xline_store.insert_deployment_yaml(&name, &yaml).await?;
            } else {
                let new_deploy = *incoming_deploy;
                let yaml = serde_yaml::to_string(&new_deploy)?;
                xline_store.insert_deployment_yaml(&name, &yaml).await?;
            }
            conn.send_msg(&RksMessage::Ack).await?;
        }

        RksMessage::DeleteDeployment(name) => {
            xline_store
                .delete_object(
                    common::ResourceKind::Deployment,
                    &name,
                    common::DeletePropagationPolicy::Background,
                )
                .await?;
            info!(
                target: "rks::node::user_dispatch",
                "marked Deployment {} for deletion (background policy)",
                name
            );
            conn.send_msg(&RksMessage::Ack).await?;
        }

        RksMessage::GetDeployment(name) => {
            if let Some(deploy) = xline_store.get_deployment(&name).await? {
                info!(
                    target: "rks::node::user_dispatch",
                    "retrieved Deployment {name}"
                );
                conn.send_msg(&RksMessage::GetDeploymentRes(Box::new(deploy)))
                    .await?;
            } else {
                conn.send_msg(&RksMessage::Error(format!("Deployment {} not found", name)))
                    .await?;
            }
        }

        RksMessage::ListDeployment => {
            let deps = xline_store.list_deployments().await?;
            info!(
                target: "rks::node::user_dispatch",
                "list current deployments: {} items",
                deps.len()
            );
            conn.send_msg(&RksMessage::ListDeploymentRes(deps)).await?;
        }

        RksMessage::RollbackDeployment { name, revision } => {
            use crate::controllers::deployment::DeploymentController;
            let controller = DeploymentController::new(xline_store.clone());
            match controller.rollback_to_revision(&name, revision).await {
                Ok(()) => {
                    info!(
                        target: "rks::node::user_dispatch",
                        "rolled back Deployment {} to revision {}",
                        name,
                        if revision == 0 { "previous".to_string() } else { revision.to_string() }
                    );
                    conn.send_msg(&RksMessage::Ack).await?;
                }
                Err(e) => {
                    conn.send_msg(&RksMessage::Error(format!("Rollback failed: {}", e)))
                        .await?;
                }
            }
        }

        RksMessage::GetDeploymentHistory(name) => {
            use crate::controllers::deployment::DeploymentController;
            let controller = DeploymentController::new(xline_store.clone());
            match controller.get_deployment_revision_history(&name).await {
                Ok(history) => {
                    let history_info: Vec<common::DeploymentRevisionInfo> = history
                        .into_iter()
                        .map(|h| common::DeploymentRevisionInfo {
                            revision: h.revision,
                            revision_history: h.revision_history,
                            replicaset_name: h.replicaset_name,
                            created_at: h.created_at,
                            replicas: h.replicas,
                            image: h.image,
                            is_current: h.is_current,
                        })
                        .collect();
                    info!(
                        target: "rks::node::user_dispatch",
                        "retrieved Deployment {} history: {} revisions",
                        name,
                        history_info.len()
                    );
                    conn.send_msg(&RksMessage::DeploymentHistoryRes(history_info))
                        .await?;
                }
                Err(e) => {
                    conn.send_msg(&RksMessage::Error(format!("Failed to get history: {}", e)))
                        .await?;
                }
            }
        }

        RksMessage::GetNodeCount => {
            info!(
                target: "rks::node::user_dispatch",
                "GetNodeCount received"
            );
        }
        RksMessage::UpdatePodStatus {
            pod_name,
            pod_namespace,
            status,
        } => {
            info!(
                target: "rks::node::user_dispatch",
                "UpdatePodStatus received for Pod {}/{}", pod_namespace, pod_name
            );
            // Update the pod status in xline store
            if let Some(pod_yaml) = xline_store.get_pod_yaml(&pod_name).await? {
                let mut pod_task: PodTask = serde_yaml::from_str(&pod_yaml)?;
                pod_task.status = status;
                let new_yaml = serde_yaml::to_string(&pod_task)?;
                xline_store.insert_pod_yaml(&pod_name, &new_yaml).await?;
                info!(
                    target: "rks::node::user_dispatch",
                    "updated PodTask {}/{} status", pod_namespace, pod_name
                );
                conn.send_msg(&RksMessage::Ack).await?;
            } else {
                warn!(
                    target: "rks::node::user_dispatch",
                    "PodTask {}/{} not found when updating status", pod_namespace, pod_name
                );
                conn.send_msg(&RksMessage::Error(format!(
                    "PodTask {}/{} not found",
                    pod_namespace, pod_name
                )))
                .await?;
            }
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
