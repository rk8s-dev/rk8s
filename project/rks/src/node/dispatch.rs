use crate::api::xlinestore::XlineStore;
use crate::commands::{create, delete};
use crate::network::service_ip::{
    validate_and_allocate_cluster_ip, validate_cluster_ip_immutability,
};
use crate::node::Shared;
use chrono::Utc;
use common::quic::RksConnection;
use common::*;
use common::{Node, NodeStatus, PodTask, RksMessage, ServiceSpec};
use log::{error, info, warn};
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::time::{Duration, sleep};

/// Dispatch worker-originated messages
pub async fn dispatch_worker(
    msg: RksMessage,
    conn: &RksConnection,
    xline_store: &Arc<XlineStore>,
    shared: &Arc<Shared>,
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
        RksMessage::PodLogsChunk {
            ref namespace,
            ref pod_name,
            ref data,
            is_final,
        } => {
            if is_final {
                info!(
                    target: "rks::node::worker_dispatch",
                    "received final log chunk for {}/{} ({} bytes)", namespace, pod_name, data.len()
                );
            }
            let log_key = format!("{}/{}", namespace, pod_name);
            shared.log_response_registry.send(&log_key, msg).await;
        }
        RksMessage::PodLogsError {
            ref namespace,
            ref pod_name,
            ref error,
        } => {
            error!(
                target: "rks::node::worker_dispatch",
                "worker reported log error for {}/{}: {}", namespace, pod_name, error
            );
            let log_key = format!("{}/{}", namespace, pod_name);
            shared.log_response_registry.send(&log_key, msg).await;
        }
        _ => warn!(
            target: "rks::node::worker_dispatch",
            "unknown or unexpected message from worker"
        ),
    }
    Ok(())
}

/// Validate Service spec before creation or update.
///
/// Checks:
/// - ClusterIP IPv4 format (if specified)
///
/// Range validation and allocation semantics are handled by
/// `validate_and_allocate_cluster_ip` for a single source of truth.
fn validate_service_spec(spec: &ServiceSpec) -> anyhow::Result<()> {
    // Only validate ClusterIP services
    if spec.service_type != "ClusterIP" {
        return Ok(());
    }

    // If cluster_ip is specified, validate format
    if let Some(ref ip_str) = spec.cluster_ip
        && !ip_str.trim().is_empty()
        && !ip_str.trim().eq_ignore_ascii_case("none")
    {
        // Try to parse as IPv4 address
        ip_str
            .trim()
            .parse::<Ipv4Addr>()
            .map_err(|_| anyhow::anyhow!("invalid cluster_ip format: {}", ip_str))?;
        info!("Service cluster_ip validation passed: {}", ip_str);
    }

    Ok(())
}

/// Handle user-originated messages
pub async fn dispatch_user(
    msg: RksMessage,
    conn: &RksConnection,
    shared: &Arc<crate::node::Shared>,
) -> anyhow::Result<()> {
    let xline_store = &shared.xline_store;
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

        // Service operations
        RksMessage::CreateService(mut svc) => {
            // Validate service spec
            if let Err(e) = validate_service_spec(&svc.spec) {
                warn!(
                    target: "rks::node::user_dispatch",
                    "Service {} validation failed: {}",
                    svc.metadata.name, e
                );
                conn.send_msg(&RksMessage::Error(format!(
                    "Service validation failed: {}",
                    e
                )))
                .await?;
                return Ok(());
            }

            // Auto-allocate ClusterIP if allocator is available
            if let Some(ref allocator) = shared.service_ip_allocator
                && let Err(e) = validate_and_allocate_cluster_ip(
                    &svc.metadata.namespace,
                    &svc.metadata.name,
                    &mut svc.spec,
                    &shared.network_config,
                    allocator.registry.as_ref(),
                    allocator.as_ref(),
                )
                .await
            {
                warn!(
                    target: "rks::node::user_dispatch",
                    "Service {} ClusterIP allocation failed: {}",
                    svc.metadata.name, e
                );
                conn.send_msg(&RksMessage::Error(format!(
                    "ClusterIP allocation failed: {}",
                    e
                )))
                .await?;
                return Ok(());
            }

            let name = svc.metadata.name.clone();
            if svc.metadata.creation_timestamp.is_none() {
                svc.metadata.creation_timestamp = Some(Utc::now());
            }
            let yaml = serde_yaml::to_string(&*svc)?;
            let created = xline_store
                .insert_service_yaml_if_absent(&name, &yaml)
                .await?;
            if !created {
                // Concurrent create won the CAS; rollback reservation from this request.
                if let Some(ref allocator) = shared.service_ip_allocator {
                    let should_release = match xline_store.get_service_yaml(&name).await {
                        Ok(Some(existing_yaml)) => {
                            if let Ok(existing_svc) =
                                serde_yaml::from_str::<ServiceTask>(&existing_yaml)
                            {
                                existing_svc.spec.cluster_ip != svc.spec.cluster_ip
                            } else {
                                true
                            }
                        }
                        Ok(None) => true,
                        Err(_) => true,
                    };

                    if should_release
                        && let Err(e) = crate::network::service_ip::deallocate_cluster_ip(
                            &name,
                            &svc.spec,
                            allocator.as_ref(),
                        )
                        .await
                    {
                        warn!(
                            target: "rks::node::user_dispatch",
                            "Failed to rollback reserved ClusterIP for concurrent create on Service {}: {}",
                            name,
                            e
                        );
                    }
                }

                conn.send_msg(&RksMessage::Error(format!(
                    "service \"{}\" already exists",
                    name
                )))
                .await?;
                return Ok(());
            }
            info!(
                target: "rks::node::user_dispatch",
                "created Service {} with ClusterIP {:?}",
                name, svc.spec.cluster_ip
            );
            conn.send_msg(&RksMessage::Ack).await?;
        }

        RksMessage::UpdateService(incoming_svc) => {
            // Validate service spec
            if let Err(e) = validate_service_spec(&incoming_svc.spec) {
                warn!(
                    target: "rks::node::user_dispatch",
                    "Service {} validation failed: {}",
                    incoming_svc.metadata.name, e
                );
                conn.send_msg(&RksMessage::Error(format!(
                    "Service validation failed: {}",
                    e
                )))
                .await?;
                return Ok(());
            }

            let name = incoming_svc.metadata.name.clone();
            if let Some(existing_yaml) = xline_store.get_service_yaml(&name).await? {
                let mut final_svc: ServiceTask = serde_yaml::from_str(&existing_yaml)?;
                let mut desired_spec = incoming_svc.spec.clone();

                // Preserve allocated ClusterIP when client manifest omits cluster_ip.
                let incoming_cluster_ip_missing = desired_spec
                    .cluster_ip
                    .as_ref()
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(true);
                if incoming_cluster_ip_missing {
                    desired_spec.cluster_ip = final_svc.spec.cluster_ip.clone();
                }

                // Validate cluster_ip immutability
                if let Err(e) =
                    validate_cluster_ip_immutability(&name, &final_svc.spec, &desired_spec)
                {
                    warn!(
                        target: "rks::node::user_dispatch",
                        "Service {} update rejected: {}",
                        name, e
                    );
                    conn.send_msg(&RksMessage::Error(format!(
                        "Service update rejected: {}",
                        e
                    )))
                    .await?;
                    return Ok(());
                }

                // Re-run allocation/validation on update path to ensure:
                // 1) missing legacy cluster_ip gets repaired,
                // 2) manual cluster_ip is validated and tracked in registry.
                if let Some(ref allocator) = shared.service_ip_allocator
                    && let Err(e) = validate_and_allocate_cluster_ip(
                        &final_svc.metadata.namespace,
                        &name,
                        &mut desired_spec,
                        &shared.network_config,
                        allocator.registry.as_ref(),
                        allocator.as_ref(),
                    )
                    .await
                {
                    warn!(
                        target: "rks::node::user_dispatch",
                        "Service {} ClusterIP validation/allocation failed on update: {}",
                        name,
                        e
                    );
                    conn.send_msg(&RksMessage::Error(format!(
                        "ClusterIP validation/allocation failed: {}",
                        e
                    )))
                    .await?;
                    return Ok(());
                }

                if final_svc.spec != desired_spec {
                    let current_gen = final_svc.metadata.generation.unwrap_or(0);
                    final_svc.metadata.generation = Some(current_gen + 1);
                    final_svc.spec = desired_spec;
                    info!(
                        target: "rks::node::user_dispatch",
                        "Service {} spec updated, add generation",
                        name
                    );
                }
                let yaml = serde_yaml::to_string(&final_svc)?;
                xline_store.insert_service_yaml(&name, &yaml).await?;
                info!(
                    target: "rks::node::user_dispatch",
                    "updated Service {} (preserved state)",
                    name
                );
            } else {
                let mut new_svc = (*incoming_svc).clone();

                if let Some(ref allocator) = shared.service_ip_allocator
                    && let Err(e) = validate_and_allocate_cluster_ip(
                        &new_svc.metadata.namespace,
                        &new_svc.metadata.name,
                        &mut new_svc.spec,
                        &shared.network_config,
                        allocator.registry.as_ref(),
                        allocator.as_ref(),
                    )
                    .await
                {
                    warn!(
                        target: "rks::node::user_dispatch",
                        "Service {} ClusterIP allocation failed on upsert-create: {}",
                        new_svc.metadata.name,
                        e
                    );
                    conn.send_msg(&RksMessage::Error(format!(
                        "ClusterIP allocation failed: {}",
                        e
                    )))
                    .await?;
                    return Ok(());
                }

                if new_svc.metadata.creation_timestamp.is_none() {
                    new_svc.metadata.creation_timestamp = Some(Utc::now());
                }

                let yaml = serde_yaml::to_string(&new_svc)?;
                let created = xline_store
                    .insert_service_yaml_if_absent(&name, &yaml)
                    .await?;
                if !created {
                    // Concurrent apply/create won the CAS; avoid leaving orphan IP reservation.
                    if let Some(ref allocator) = shared.service_ip_allocator {
                        let should_release = match xline_store.get_service_yaml(&name).await {
                            Ok(Some(existing_yaml)) => {
                                if let Ok(existing_svc) =
                                    serde_yaml::from_str::<ServiceTask>(&existing_yaml)
                                {
                                    existing_svc.spec.cluster_ip != new_svc.spec.cluster_ip
                                } else {
                                    true
                                }
                            }
                            Ok(None) => true,
                            Err(_) => true,
                        };

                        if should_release
                            && let Err(e) = crate::network::service_ip::deallocate_cluster_ip(
                                &name,
                                &new_svc.spec,
                                allocator.as_ref(),
                            )
                            .await
                        {
                            warn!(
                                target: "rks::node::user_dispatch",
                                "Failed to rollback reserved ClusterIP for concurrent apply on Service {}: {}",
                                name,
                                e
                            );
                        }
                    }

                    conn.send_msg(&RksMessage::Error(format!(
                        "Service {} was created concurrently, please retry apply",
                        name
                    )))
                    .await?;
                    return Ok(());
                }
            }
            conn.send_msg(&RksMessage::Ack).await?;
        }

        RksMessage::DeleteService(name) => {
            let existing_svc = xline_store
                .get_service_yaml(&name)
                .await?
                .and_then(|yaml| serde_yaml::from_str::<ServiceTask>(&yaml).ok());

            xline_store
                .delete_object(
                    common::ResourceKind::Service,
                    &name,
                    common::DeletePropagationPolicy::Background,
                )
                .await?;
            info!(
                target: "rks::node::user_dispatch",
                "marked Service {} for deletion (background policy)",
                name
            );

            // Release IP only after Service object is actually removed.
            if let Some(ref allocator) = shared.service_ip_allocator
                && let Some(svc) = existing_svc
            {
                let xline_store = xline_store.clone();
                let allocator = allocator.clone();
                let service_name = name.clone();

                tokio::spawn(async move {
                    for _ in 0..300 {
                        match xline_store.get_service_yaml(&service_name).await {
                            Ok(None) => {
                                if let Err(e) = crate::network::service_ip::deallocate_cluster_ip(
                                    &service_name,
                                    &svc.spec,
                                    allocator.as_ref(),
                                )
                                .await
                                {
                                    warn!(
                                        target: "rks::node::user_dispatch",
                                        "Failed to deallocate ClusterIP for Service {} after final delete: {}",
                                        service_name,
                                        e
                                    );
                                }
                                return;
                            }
                            Ok(Some(_)) => {
                                sleep(Duration::from_secs(1)).await;
                            }
                            Err(e) => {
                                warn!(
                                    target: "rks::node::user_dispatch",
                                    "Failed to verify Service {} deletion before IP release: {}",
                                    service_name,
                                    e
                                );
                                sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }

                    info!(
                        target: "rks::node::user_dispatch",
                        "Service {} not fully deleted within timeout, keep ClusterIP reserved",
                        service_name
                    );
                });
            }

            conn.send_msg(&RksMessage::Ack).await?;
        }

        RksMessage::GetService(name) => {
            if let Some(svc) = xline_store.get_service(&name).await? {
                info!(
                    target: "rks::node::user_dispatch",
                    "retrieved Service {name}"
                );
                conn.send_msg(&RksMessage::GetServiceRes(Box::new(svc)))
                    .await?;
            } else {
                conn.send_msg(&RksMessage::Error(format!("Service {} not found", name)))
                    .await?;
            }
        }

        RksMessage::ListService => {
            let services = xline_store.list_services().await?;
            info!(
                target: "rks::node::user_dispatch",
                "list current services: {} items",
                services.len()
            );
            conn.send_msg(&RksMessage::ListServiceRes(services)).await?;
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
            mut status,
        } => {
            info!(
                target: "rks::node::user_dispatch",
                "UpdatePodStatus received for Pod {}/{}", pod_namespace, pod_name
            );
            // Update the pod status in xline store
            if let Some(pod_yaml) = xline_store.get_pod_yaml(&pod_name).await? {
                let mut pod_task: PodTask = serde_yaml::from_str(&pod_yaml)?;
                // Preserve existing pod_ip if the incoming status does not carry it.
                // This avoids wiping pod_ip set by SetPodip.
                if status.pod_ip.is_none() {
                    status.pod_ip = pod_task.status.pod_ip.clone();
                }
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
        RksMessage::GetPodLogs {
            pod_name,
            namespace,
            container_name,
            follow,
            tail_lines,
            since_time,
            timestamps,
            previous,
        } => {
            info!(
                target: "rks::node::user_dispatch",
                "GetPodLogs received for Pod {}/{}", namespace, pod_name
            );

            // Query Xline to get pod spec and find assigned node
            let pod = match xline_store.get_pod(&pod_name).await? {
                Some(pod) => pod,
                None => {
                    conn.send_msg(&RksMessage::PodLogsError {
                        namespace: namespace.clone(),
                        pod_name: pod_name.clone(),
                        error: format!("Pod {} not found", pod_name),
                    })
                    .await?;
                    return Ok(());
                }
            };

            let node_name = match &pod.spec.node_name {
                Some(name) => name,
                None => {
                    conn.send_msg(&RksMessage::PodLogsError {
                        namespace: namespace.clone(),
                        pod_name: pod_name.clone(),
                        error: format!("Pod {} is not assigned to any node yet", pod_name),
                    })
                    .await?;
                    return Ok(());
                }
            };

            // Look up node in NodeRegistry
            let worker_session = match shared.node_registry.get(node_name).await {
                Some(session) => session,
                None => {
                    conn.send_msg(&RksMessage::PodLogsError {
                        namespace: namespace.clone(),
                        pod_name: pod_name.clone(),
                        error: format!("Worker node {} is not connected", node_name),
                    })
                    .await?;
                    return Ok(());
                }
            };

            // Register response channel and forward log request to worker node
            let log_key = format!("{}/{}", namespace, pod_name);
            let mut rx = shared.log_response_registry.register(log_key).await;

            let log_request = RksMessage::GetPodLogs {
                pod_name: pod_name.clone(),
                namespace: namespace.clone(),
                container_name: container_name.clone(),
                follow,
                tail_lines,
                since_time: since_time.clone(),
                timestamps,
                previous,
            };

            if let Err(e) = worker_session.tx.send(log_request).await {
                conn.send_msg(&RksMessage::PodLogsError {
                    namespace: namespace.clone(),
                    pod_name: pod_name.clone(),
                    error: format!("Failed to forward log request to worker: {}", e),
                })
                .await?;
                return Ok(());
            }

            info!(
                target: "rks::node::user_dispatch",
                "Forwarded log request for Pod {}/{} to worker node {}",
                namespace, pod_name, node_name
            );

            loop {
                match rx.recv().await {
                    Some(RksMessage::PodLogsChunk {
                        namespace,
                        pod_name,
                        data,
                        is_final,
                    }) => {
                        conn.send_msg(&RksMessage::PodLogsChunk {
                            namespace,
                            pod_name,
                            data,
                            is_final,
                        })
                        .await?;
                        if is_final {
                            break;
                        }
                    }
                    Some(RksMessage::PodLogsError {
                        namespace,
                        pod_name,
                        error,
                    }) => {
                        conn.send_msg(&RksMessage::PodLogsError {
                            namespace,
                            pod_name,
                            error,
                        })
                        .await?;
                        break;
                    }
                    Some(other) => {
                        warn!(
                            target: "rks::node::user_dispatch",
                            "unexpected message in log channel: {:?}", other
                        );
                    }
                    None => {
                        // Channel closed without final chunk
                        conn.send_msg(&RksMessage::PodLogsChunk {
                            namespace: namespace.clone(),
                            pod_name: pod_name.clone(),
                            data: vec![],
                            is_final: true,
                        })
                        .await?;
                        break;
                    }
                }
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
