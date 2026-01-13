use std::sync::Arc;

use chrono::{DateTime, Utc};
use common::{
    ConditionStatus, ContainerState, ContainerStatus, PodCondition, PodConditionType, PodPhase,
    PodSpec, PodStatus, PodTask, RestartPolicy, RksMessage,
};
use dashmap::DashMap;
use tokio::sync::{Notify, OnceCell};
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    commands::pod::TLSConnectionArgs,
    daemon::{
        status::get_pod_by_uid,
        sync_loop::{Event, WithEvent},
    },
    quic::client::{Daemon, QUICClient},
};

const SYNC_DURATION: std::time::Duration = std::time::Duration::from_secs(5);

pub static STATUS_MANAGER: OnceCell<Arc<StatusManager>> = OnceCell::const_new();

#[derive(Debug, Clone)]
struct VersionedPodStatus {
    version: u64,
    status: PodStatus,
    pod_name: String,
    pod_namespace: String,
    pod_is_finished: bool,
    at: DateTime<Utc>,
}

impl Default for VersionedPodStatus {
    fn default() -> Self {
        VersionedPodStatus {
            version: 0,
            status: PodStatus::default(),
            pod_name: String::new(),
            pod_namespace: String::new(),
            pod_is_finished: false,
            at: chrono::DateTime::<Utc>::from_timestamp_millis(0).unwrap_or_else(chrono::Utc::now),
        }
    }
}

pub struct StatusManager {
    client: QUICClient<Daemon>,
    pod_statuses: Arc<DashMap<Uuid, VersionedPodStatus>>,
    pod_status_update_signal: Arc<Notify>,
    api_status_versions: Arc<DashMap<Uuid, u64>>,
    sync_loop_handle: Option<tokio::task::JoinHandle<anyhow::Result<()>>>,
}

struct State {
    client: QUICClient<Daemon>,
    pod_statuses: Arc<DashMap<Uuid, VersionedPodStatus>>,
    pod_status_update_signal: Arc<Notify>,
    api_status_versions: Arc<DashMap<Uuid, u64>>,
}

impl StatusManager {
    pub async fn try_new(
        server_addr: String,
        tls_cfg: Arc<TLSConnectionArgs>,
    ) -> anyhow::Result<Self> {
        let client = QUICClient::<Daemon>::connect(&server_addr, &tls_cfg).await?;
        let pod_statuses = Arc::new(DashMap::new());
        let pod_status_update_signal = Arc::new(Notify::new());
        let api_status_versions = Arc::new(DashMap::new());
        Ok(StatusManager {
            client,
            pod_statuses,
            pod_status_update_signal,
            api_status_versions,
            sync_loop_handle: None,
        })
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        info!("Starting to sync pod status with rks.");

        let state = Arc::new(State {
            client: self.client.clone(),
            pod_statuses: self.pod_statuses.clone(),
            pod_status_update_signal: self.pod_status_update_signal.clone(),
            api_status_versions: self.api_status_versions.clone(),
        });

        self.sync_loop_handle = Some(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(SYNC_DURATION);
            loop {
                tokio::select! {
                    _ = state.pod_status_update_signal.notified() => {
                        // Sync on-demand
                        info!("Syncing updated pod status.");
                        if let Err(e) = sync_batch(&state, false).await {
                            error!("Failed to sync updated pod statuses: {e}");
                        }
                    }
                    _ = ticker.tick() => {
                        // Periodic sync all
                        info!("Syncing all pod statuses.");
                        if let Err(e) = sync_batch(&state, true).await {
                            error!("Failed to sync all pod statuses: {e}");
                        }
                    }
                }
            }
        }));
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(handle) = self.sync_loop_handle.take() {
            handle.abort();
        }
        Ok(())
    }

    pub async fn set_pod_status(&self, pod: &PodTask, status: &PodStatus) -> anyhow::Result<()> {
        self.update_status_internal(
            pod,
            status,
            pod.metadata.deletion_timestamp.is_some(),
            false,
        )
        .await?;
        Ok(())
    }

    pub async fn get_pod_status(&self, pod_uid: Uuid) -> Option<PodStatus> {
        self.pod_statuses.get(&pod_uid).map(|p| p.status.clone())
    }

    pub async fn set_container_readiness(
        &self,
        pod_uid: Uuid,
        container_name: &str,
        is_ready: bool,
    ) -> anyhow::Result<()> {
        let pod = match get_pod_by_uid(&self.client, &pod_uid).await? {
            Some(p) => p,
            None => {
                info!(
                    "Pod with UID '{}' not found on rks, skipping container readiness update.",
                    pod_uid
                );
                return Ok(());
            }
        };

        let (is_cached, mut cached_status) = match self.pod_statuses.get(&pod_uid) {
            Some(s) => (true, s.value().clone()),
            None => (false, VersionedPodStatus::default()),
        };

        if !is_cached {
            info!("Container readiness changed before pod has synced",);
            return Ok(());
        }

        let container_status = cached_status
            .status
            .container_statuses
            .iter_mut()
            .find(|container_status| container_status.name == container_name);

        if container_status.is_none() {
            info!(
                "Container '{}' not found in pod '{}', skipping container readiness update.",
                container_name, cached_status.pod_name
            );
            return Ok(());
        }

        let container_status = container_status.unwrap();

        if container_status.ready == is_ready {
            info!(
                "Container readiness for '{}' already set to {}, skipping update.",
                container_name, is_ready
            );
            return Ok(());
        }
        container_status.ready = is_ready;

        // updates the corresponding type of condition
        let mut update_condition = |condition_type: PodConditionType, condition: &PodCondition| {
            if let Some(conditions) = cached_status.status.conditions.as_mut() {
                if let Some(idx) = conditions
                    .iter()
                    .position(|c| c.condition_type == condition_type)
                {
                    conditions[idx] = condition.clone();
                } else {
                    conditions.push(condition.clone());
                }
            } else {
                cached_status.status.conditions = Some(vec![condition.clone()]);
            }
        };

        update_condition(
            PodConditionType::ContainersReady,
            &create_containers_ready_condition(
                &pod,
                &cached_status.status.container_statuses,
                cached_status.status.phase.unwrap_or(PodPhase::Unknown),
            ),
        );

        update_condition(
            PodConditionType::PodReady,
            &create_pod_ready_condition(
                &pod,
                &cached_status.status.container_statuses,
                cached_status.status.phase.unwrap_or(PodPhase::Unknown),
            ),
        );

        self.update_status_internal(&pod, &cached_status.status, false, false)
            .await?;

        Ok(())
    }

    async fn update_status_internal(
        &self,
        pod: &PodTask,
        status: &PodStatus,
        force_update: bool,
        pod_is_finished: bool,
    ) -> anyhow::Result<()> {
        let pod_uid = pod.metadata.uid;
        let mut status = status.clone();

        let (is_cached, cached_status, old_status) = match self.pod_statuses.get(&pod_uid) {
            Some(s) => {
                let cached_status = s.value().clone();
                let old_status = cached_status.status.clone();
                (true, cached_status, old_status)
            }
            None => (false, VersionedPodStatus::default(), pod.status.clone()),
        };

        if let Err(e) = check_container_status_transition(&old_status, &status, &pod.spec) {
            error!(
                "Illegal container status transition detected for pod '{}': {e}",
                pod.metadata.name
            );
            return Ok(());
        }

        update_last_transition_time(&old_status, &mut status, &PodConditionType::PodReady)?;

        update_last_transition_time(&old_status, &mut status, &PodConditionType::ContainersReady)?;

        update_last_transition_time(&old_status, &mut status, &PodConditionType::PodInitialized)?;

        update_last_transition_time(&old_status, &mut status, &PodConditionType::PodScheduled)?;

        if let Some(start_time) = old_status.start_time {
            status.start_time = Some(start_time);
        }

        //
        if is_cached && is_status_owned_by_rkl_equal(&old_status, &status) && !force_update {
            return Ok(());
        }

        let new_status = VersionedPodStatus {
            status,
            version: cached_status.version + 1,
            pod_name: pod.metadata.name.clone(),
            pod_namespace: pod.metadata.namespace.clone(),
            pod_is_finished,
            at: if cached_status.at < Utc::now() {
                Utc::now()
            } else {
                cached_status.at
            },
        };

        // Update the status in the cache.
        self.pod_statuses.insert(pod_uid, new_status);

        // Notify the main loop to process the updated status.
        self.pod_status_update_signal.notify_one();
        Ok(())
    }
}

impl Drop for StatusManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

async fn sync_batch(state: &Arc<State>, sync_all: bool) -> anyhow::Result<()> {
    let mut updated_status: Vec<(Uuid, VersionedPodStatus)> = Vec::new();

    // Clean up orphaned versions.
    if sync_all {
        for entry in state.api_status_versions.iter() {
            let uid = *entry.key();
            let has_pod = state.pod_statuses.get(&uid).is_some();
            if !has_pod {
                state.api_status_versions.remove(&uid);
            }
        }
    }

    // Decide which pods need status updates.
    for entry in state.pod_statuses.iter() {
        let pod_uid = *entry.key();
        let pod_status = entry.value().clone();

        if !sync_all {
            if let Some(api_version) = state.api_status_versions.get(&pod_uid)
                && *api_version.value() >= pod_status.version
            {
                continue;
            }

            updated_status.push((pod_uid, pod_status));
            continue;
        }

        if need_update(state, &pod_uid, &pod_status).await? {
            updated_status.push((pod_uid, pod_status));
        } else if need_reconcile(state, &pod_uid, &pod_status).await {
            state.api_status_versions.remove(&pod_uid);
            updated_status.push((pod_uid, pod_status));
        }
    }

    for (pod_uid, pod_status) in updated_status {
        info!(
            "Sync status for pod '{}'(version {})",
            pod_status.pod_name, pod_status.version
        );
        sync_pod(state, pod_uid, &pod_status).await?;
    }

    Ok(())
}

async fn sync_pod(
    state: &State,
    pod_uid: Uuid,
    pod_status: &VersionedPodStatus,
) -> anyhow::Result<()> {
    let pod = match get_pod_by_uid(&state.client, &pod_uid).await? {
        Some(p) => p,
        None => {
            info!(
                "Pod with UID '{}' not found on server, skipping status sync.",
                pod_uid
            );
            return Ok(());
        }
    };

    let merged_status = merge_status(&pod.status, &pod_status.status).await;

    // Update the pod status on the server
    update_pod_status(
        &state.client,
        &pod.metadata.name,
        &pod.metadata.namespace,
        &merged_status,
    )
    .await?;

    // After successful update, record the latest version
    state
        .api_status_versions
        .insert(pod_uid, pod_status.version);

    Ok(())
}

async fn update_pod_status(
    client: &QUICClient<Daemon>,
    pod_name: &str,
    pod_namespace: &str,
    pod_status: &PodStatus,
) -> anyhow::Result<()> {
    client
        .send_msg(&RksMessage::UpdatePodStatus {
            pod_name: pod_name.to_string(),
            pod_namespace: pod_namespace.to_string(),
            status: pod_status.clone(),
        })
        .await?;

    match client.fetch_msg().await? {
        RksMessage::Ack => Ok(()),
        RksMessage::Error(err_msg) => Err(anyhow::anyhow!(
            "Failed to upload pod status for '{}': {}",
            pod_name,
            err_msg
        )),
        _ => Err(anyhow::anyhow!(
            "Unexpected response when uploading pod status for '{}'",
            pod_name
        )),
    }
}

async fn merge_status(old_pod_status: &PodStatus, new_pod_status: &PodStatus) -> PodStatus {
    let mut merged_status = new_pod_status.clone();

    let mut pod_conditions: Vec<_> = Vec::new();

    for pod_condition in old_pod_status.conditions.as_ref().unwrap_or(&Vec::new()) {
        if !condition_type_owned_by_rkl(&pod_condition.condition_type) {
            pod_conditions.push(pod_condition.clone());
        }
    }

    for pod_condition in new_pod_status.conditions.as_ref().unwrap_or(&Vec::new()) {
        if condition_type_owned_by_rkl(&pod_condition.condition_type) {
            pod_conditions.push(pod_condition.clone());
        }
    }

    merged_status.conditions = Some(pod_conditions);

    // If the new phase is terminal, explicitly set the ready condition to false for PodReady and ContainersReady.
    if is_pod_phase_terminal(new_pod_status.phase.as_ref().unwrap_or(&PodPhase::Unknown))
        && (get_pod_ready_condition(new_pod_status).is_some()
            || get_container_ready_condition(new_pod_status).is_some())
    {
        let ready_condition = PodCondition {
            condition_type: PodConditionType::PodReady,
            status: common::ConditionStatus::False,
            reason: if let Some(phase) = &new_pod_status.phase {
                match phase {
                    PodPhase::Succeeded => Some("PodCompleted".to_string()),
                    PodPhase::Failed => Some("PodFailed".to_string()),
                    _ => Some("Unknown".to_string()),
                }
            } else {
                Some("Unknown".to_string())
            },
            ..Default::default()
        };

        update_pod_condition(&mut merged_status, ready_condition);

        let containers_ready_condition = PodCondition {
            condition_type: PodConditionType::ContainersReady,
            status: common::ConditionStatus::False,
            reason: if let Some(phase) = &new_pod_status.phase {
                match phase {
                    PodPhase::Succeeded => Some("PodCompleted".to_string()),
                    PodPhase::Failed => Some("PodFailed".to_string()),
                    _ => Some("Unknown".to_string()),
                }
            } else {
                Some("Unknown".to_string())
            },
            ..Default::default()
        };

        update_pod_condition(&mut merged_status, containers_ready_condition);
    }

    merged_status
}

fn condition_type_owned_by_rkl(condition_type: &common::PodConditionType) -> bool {
    matches!(
        condition_type,
        PodConditionType::PodScheduled
            | PodConditionType::PodReady
            | PodConditionType::PodInitialized
            | PodConditionType::ContainersReady
    )
}

/// Determine whether the status is stale for the given pod uid.
async fn need_update(
    state: &Arc<State>,
    pod_uid: &Uuid,
    pod_status: &VersionedPodStatus,
) -> anyhow::Result<bool> {
    let latest_api_version = match state.api_status_versions.get(pod_uid) {
        Some(v) => *v.value(),
        None => return Ok(true),
    };

    if latest_api_version < pod_status.version {
        return Ok(true);
    }

    let pod = match get_pod_by_uid(&state.client, pod_uid).await? {
        Some(p) => p,
        None => return Ok(false),
    };

    can_be_deleted(pod_status, &pod)
}

fn can_be_deleted(local_status: &VersionedPodStatus, remote_pod: &PodTask) -> anyhow::Result<bool> {
    if remote_pod.metadata.deletion_timestamp.is_none() {
        return Ok(false);
    }

    if !is_pod_phase_terminal(
        remote_pod
            .status
            .phase
            .as_ref()
            .unwrap_or(&PodPhase::Unknown),
    ) {
        return Ok(false);
    }

    if local_status.pod_is_finished {
        return Ok(true);
    }

    Ok(false)
}

async fn need_reconcile(
    state: &Arc<State>,
    pod_uid: &Uuid,
    pod_status: &VersionedPodStatus,
) -> bool {
    let pod_option = get_pod_by_uid(&state.client, pod_uid).await.ok().flatten();
    if pod_option.is_none() {
        return false;
    }
    let pod = pod_option.unwrap();

    if pod_status.status == pod.status {
        return false;
    }

    info!(
        "Pod status mismatch detected for pod '{}', need reconcile. Local status: {:?}, Remote status: {:?}.",
        pod.metadata.name, pod_status.status, pod.status
    );

    true
}

/// Ensures that no container is trying to transition
// from a terminated to non-terminated state, which is illegal and indicates a logical error
fn check_container_status_transition(
    old_status: &PodStatus,
    new_status: &PodStatus,
    pod_spec: &PodSpec,
) -> anyhow::Result<()> {
    // if always restart, containers are allowed to transition from terminated to non-terminated
    if pod_spec.restart_policy == RestartPolicy::Always {
        return Ok(());
    }

    for old_status in &old_status.container_statuses {
        if let Some(ContainerState::Terminated { exit_code, .. }) = old_status.state
            && exit_code != 0
            && pod_spec.restart_policy == RestartPolicy::OnFailure
        {
            continue;
        }

        for new_status in &new_status.container_statuses {
            if old_status.name == new_status.name
                && !matches!(new_status.state, Some(ContainerState::Terminated { .. }))
            {
                return Err(anyhow::anyhow!(
                    "Illegal container status transition detected for container '{}': cannot transition from Terminated to non-Terminated state.",
                    old_status.name
                ));
            }
        }
    }

    Ok(())
}

fn is_pod_phase_terminal(phase: &PodPhase) -> bool {
    matches!(phase, PodPhase::Succeeded | PodPhase::Failed)
}

fn get_pod_ready_condition(status: &PodStatus) -> Option<&PodCondition> {
    if let Some((_, condition)) = get_pod_condition(status, &PodConditionType::PodReady) {
        Some(condition)
    } else {
        None
    }
}

fn get_container_ready_condition(status: &PodStatus) -> Option<&PodCondition> {
    if let Some((_, condition)) = get_pod_condition(status, &PodConditionType::ContainersReady) {
        Some(condition)
    } else {
        None
    }
}

/// Gets the pod condition of the specified type from the pod status.
/// Returns index and condition if found, None otherwise.
fn get_pod_condition<'a>(
    status: &'a PodStatus,
    condition_type: &PodConditionType,
) -> Option<(usize, &'a PodCondition)> {
    if let Some(conditions) = &status.conditions {
        for (index, condition) in conditions.iter().enumerate() {
            if &condition.condition_type == condition_type {
                return Some((index, condition));
            }
        }
    }

    None
}

fn get_pod_condition_mut<'a>(
    status: &'a mut PodStatus,
    condition_type: &PodConditionType,
) -> Option<(usize, &'a mut PodCondition)> {
    if let Some(conditions) = &mut status.conditions {
        for (index, condition) in conditions.iter_mut().enumerate() {
            if &condition.condition_type == condition_type {
                return Some((index, condition));
            }
        }
    }

    None
}

/// Updates existing pod condition or creates a new one. Sets LastTransitionTime to now if the status has changed.
/// Returns true if pod condition has changed or has been added.
fn update_pod_condition(status: &mut PodStatus, new_condition: PodCondition) -> bool {
    let now = chrono::Utc::now();
    let old_condition_opt = get_pod_condition(status, &new_condition.condition_type);
    match old_condition_opt {
        Some((index, old_condition)) => {
            if old_condition.status != new_condition.status {
                let mut updated_condition = new_condition.clone();
                updated_condition.last_transition_time = Some(now);
                if let Some(conditions) = &mut status.conditions {
                    conditions[index] = updated_condition;
                }
                true
            } else {
                false
            }
        }
        None => {
            let mut condition_to_add = new_condition.clone();
            condition_to_add.last_transition_time = Some(now);
            if let Some(conditions) = &mut status.conditions {
                conditions.push(condition_to_add);
            } else {
                status.conditions = Some(vec![condition_to_add]);
            }
            true
        }
    }
}

fn update_last_transition_time(
    old_status: &PodStatus,
    status: &mut PodStatus,
    condition_type: &PodConditionType,
) -> anyhow::Result<()> {
    let Some((_, new_condition)) = get_pod_condition_mut(status, condition_type) else {
        return Ok(());
    };

    let last_transition_time = match get_pod_condition(old_status, condition_type) {
        Some((_, old_condition)) if old_condition.status == new_condition.status => old_condition
            .last_transition_time
            .unwrap_or_else(chrono::Utc::now),
        _ => chrono::Utc::now(),
    };

    new_condition.last_transition_time = Some(last_transition_time);

    Ok(())
}

/// Check if the given pod statuses are equal when non-rkl-owned pod conditions are excluded.
fn is_status_owned_by_rkl_equal(old_status: &PodStatus, status: &PodStatus) -> bool {
    let mut filtered_old_conditions: Vec<PodCondition> = Vec::new();
    let mut filtered_new_conditions: Vec<PodCondition> = Vec::new();

    if let Some(conditions) = &old_status.conditions {
        filtered_old_conditions = conditions
            .iter()
            .filter(|c| condition_type_owned_by_rkl(&c.condition_type))
            .cloned()
            .collect();
    }

    if let Some(conditions) = &status.conditions {
        filtered_new_conditions = conditions
            .iter()
            .filter(|c| condition_type_owned_by_rkl(&c.condition_type))
            .cloned()
            .collect();
    }

    // first check conditions
    if filtered_old_conditions.len() != filtered_new_conditions.len() {
        return false;
    }

    for new_cond in &filtered_new_conditions {
        if let Some(old_cond) = filtered_old_conditions
            .iter()
            .find(|c| c.condition_type == new_cond.condition_type)
        {
            if old_cond.status != new_cond.status
                || old_cond.reason != new_cond.reason
                || old_cond.message != new_cond.message
            {
                return false;
            }
        } else {
            return false;
        }
    }

    // then check other fields
    let old_copy = PodStatus {
        conditions: status.conditions.clone(),
        ..old_status.clone()
    };

    old_copy == *status
}

fn find_container_status<'a>(
    status: &'a PodStatus,
    container_name: &str,
) -> Option<&'a common::ContainerStatus> {
    status
        .container_statuses
        .iter()
        .find(|&container_status| container_status.name == container_name)
        .map(|v| v as _)
}

fn create_pod_ready_condition(
    pod: &PodTask,
    container_statuses: &[ContainerStatus],
    phase: PodPhase,
) -> PodCondition {
    let container_ready = create_containers_ready_condition(pod, container_statuses, phase);

    if container_ready.status != ConditionStatus::True {
        PodCondition {
            condition_type: PodConditionType::PodReady,
            status: container_ready.status,
            reason: container_ready.reason,
            message: container_ready.message,
            ..Default::default()
        }
    } else {
        PodCondition {
            condition_type: PodConditionType::PodReady,
            status: ConditionStatus::True,
            ..Default::default()
        }
    }
}

fn create_containers_ready_condition(
    pod: &PodTask,
    container_statuses: &[ContainerStatus],
    phase: PodPhase,
) -> PodCondition {
    let mut unready_containers: Vec<String> = Vec::new();
    let mut unknown_containers: Vec<String> = Vec::new();
    for container in &pod.spec.containers {
        if let Some(container_status) = get_container_status(container_statuses, &container.name) {
            if !container_status.ready {
                unready_containers.push(container.name.clone());
            }
        } else {
            unknown_containers.push(container.name.clone());
        }
    }

    let mut pod_condition = PodCondition {
        condition_type: PodConditionType::ContainersReady,
        status: ConditionStatus::False,
        ..Default::default()
    };

    if phase == PodPhase::Succeeded && unknown_containers.is_empty() {
        pod_condition.reason = Some("PodCompleted".to_string());
        return pod_condition;
    } else if phase == PodPhase::Failed {
        pod_condition.reason = Some("PodFailed".to_string());
        return pod_condition;
    }

    let mut unready_reason_msgs: Vec<String> = Vec::new();
    if !unknown_containers.is_empty() {
        unready_reason_msgs.push(format!(
            "containers with unknown status: [{}]",
            unknown_containers.join(", ")
        ));
    }

    if !unready_containers.is_empty() {
        unready_reason_msgs.push(format!(
            "containers with unready status: [{}]",
            unready_containers.join(", ")
        ));
    }

    let message = unready_reason_msgs.join(", ");
    if !message.is_empty() {
        pod_condition.reason = Some("ContainersNotReady".to_string());
        pod_condition.message = Some(message);
        pod_condition.status = ConditionStatus::False;
        pod_condition
    } else {
        pod_condition.status = ConditionStatus::True;
        pod_condition
    }
}

fn get_container_status<'a>(
    container_statuses: &'a [ContainerStatus],
    container_name: &str,
) -> Option<&'a ContainerStatus> {
    container_statuses
        .iter()
        .find(|cs| cs.name == container_name)
}
