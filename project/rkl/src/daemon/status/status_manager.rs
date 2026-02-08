//! Manages local pod status caching, deduplication, and synchronization with the rks API server.
//!
//! [`StatusManager`] caches [`PodStatus`] locally and deduplicates unchanged updates to reduce
//! unnecessary syncs. It maintains a versioned cache to track which statuses need uploading.
//! Synchronization is bidirectional: on-demand (signal-driven) syncs when status changes,
//! and periodic syncs every 5 seconds via the background sync loop.
//!
//! Before uploading to the rks API server over QUIC, [`StatusManager`] merges rkl-owned
//! conditions (PodReady, ContainersReady, PodScheduled, PodInitialized) with server-side
//! conditions to preserve pod status ownership contracts.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use common::{
    ConditionStatus, ContainerState, ContainerStatus, PodCondition, PodConditionType, PodPhase,
    PodSpec, PodStatus, PodTask, RestartPolicy, RksMessage,
};
use dashmap::DashMap;
use libcontainer::syscall::syscall::create_syscall;
use libruntime::rootpath;
use tokio::sync::{Notify, OnceCell};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    commands::pod::{PodInfo, TLSConnectionArgs},
    daemon::status::{get_pod_by_uid, probe::prober::match_container_name},
    quic::client::{Cli, QUICClient},
};

const SYNC_DURATION: std::time::Duration = std::time::Duration::from_secs(5);

/// Global singleton instance of [`StatusManager`], initialized once by the daemon at startup.
///
/// Access this via [`STATUS_MANAGER`] to get a reference to the central status cache and sync engine.
pub static STATUS_MANAGER: OnceCell<Arc<StatusManager>> = OnceCell::const_new();

#[allow(unused)]
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
            at: Utc::now(),
        }
    }
}

/// The central status cache and sync engine for the daemon.
///
/// Manages local caching of [`PodStatus`] with version tracking, deduplicates unchanged updates,
/// and synchronizes changes to the rks API server over QUIC. Implements both on-demand (signal-driven)
/// and periodic (5-second ticker) synchronization strategies.
pub struct StatusManager {
    client: QUICClient<Cli>,
    pod_statuses: Arc<DashMap<Uuid, VersionedPodStatus>>,
    pod_status_update_signal: Arc<Notify>,
    api_status_versions: Arc<DashMap<Uuid, u64>>,
    sync_loop_handle: Option<tokio::task::JoinHandle<anyhow::Result<()>>>,
}

impl std::fmt::Debug for StatusManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusManager").finish()
    }
}

struct State {
    client: QUICClient<Cli>,
    pod_statuses: Arc<DashMap<Uuid, VersionedPodStatus>>,
    pod_status_update_signal: Arc<Notify>,
    api_status_versions: Arc<DashMap<Uuid, u64>>,
}

impl StatusManager {
    /// Creates a new [`StatusManager`] with a QUIC connection to the rks API server.
    ///
    /// # Arguments
    /// * `server_addr` - The rks API server address (e.g., "127.0.0.1:6000")
    /// * `tls_cfg` - TLS configuration for the QUIC connection
    ///
    /// # Errors
    /// Returns an error if the QUIC connection fails to establish.
    pub async fn try_new(
        server_addr: String,
        tls_cfg: Arc<TLSConnectionArgs>,
    ) -> anyhow::Result<Self> {
        let client = QUICClient::<Cli>::connect(&server_addr, &tls_cfg).await?;
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

    /// Starts the background sync loop.
    ///
    /// The loop runs indefinitely, syncing pod status updates via two mechanisms:
    /// - **On-demand**: When [`set_pod_status`](Self::set_pod_status) signals a change
    /// - **Periodic**: Every 5 seconds to catch any missed updates or reconcile divergence
    ///
    pub fn run(&mut self) {
        if let Some(handle) = &self.sync_loop_handle {
            if !handle.is_finished() {
                warn!("[StatusManager] run() called while already running; ignoring.");
                return;
            }
            self.sync_loop_handle = None;
        }

        info!("[StatusManager] Starting to sync pod status to rks.");

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
                        debug!("[StatusManager] Received status update signal; syncing changed pods");
                        if let Err(e) = sync_batch(&state, false).await {
                            error!("[StatusManager] Failed to sync updated pod statuses: {e}");
                        }
                    }
                    _ = ticker.tick() => {
                        // Periodic sync all
                        debug!("[StatusManager] Periodic sync tick fired; syncing all pods");
                        if let Err(e) = sync_batch(&state, true).await {
                            error!("[StatusManager] Failed to sync all pod statuses: {e}");
                        }
                    }
                }
            }
        }));
    }

    /// Stops the background sync loop.
    ///
    /// Aborts the sync task if running. Safe to call multiple times or if `run()` was never called.
    /// After stopping, no further syncs will occur until `run()` is called again.
    pub fn stop(&mut self) {
        if let Some(handle) = self.sync_loop_handle.take() {
            handle.abort();
        }
    }

    /// Updates the cached status for a pod and signals the sync loop.
    ///
    /// Caches the [`PodStatus`] locally with version tracking and deduplicates unchanged updates.
    /// If the status has changed, notifies the background sync loop to perform an on-demand sync.
    ///
    /// # Arguments
    /// * `pod` - The [`PodTask`] whose status is being updated
    /// * `status` - The new [`PodStatus`] to cache
    ///
    /// # Errors
    /// Returns an error only when internal status-processing fails.
    /// Illegal container transitions are logged and ignored (no error is returned).
    pub async fn set_pod_status(&self, pod: &PodTask, status: &PodStatus) -> anyhow::Result<()> {
        debug!(
            pod_uid = %pod.metadata.uid,
            pod_name = %pod.metadata.name,
            pod_namespace = %pod.metadata.namespace,
            phase = ?status.phase,
            container_status_count = status.container_statuses.len(),
            force_update = false,
            pod_is_finished = %pod.metadata.deletion_timestamp.is_some(),
            "[StatusManager] set_pod_status called"
        );
        self.update_status_internal(
            pod,
            status,
            pod.metadata.deletion_timestamp.is_some(),
            false,
        )
        .await?;
        Ok(())
    }

    /// Retrieves the cached status for a pod by UID.
    ///
    /// Returns a copy of the cached [`PodStatus`] if the pod UID is in the cache, or None if not found.
    ///
    /// # Arguments
    /// * `pod_uid` - The UUID of the pod to look up
    pub async fn get_pod_status(&self, pod_uid: Uuid) -> Option<PodStatus> {
        self.pod_statuses.get(&pod_uid).map(|p| p.status.clone())
    }

    /// Updates a specific container's readiness and recalculates PodReady and ContainersReady conditions.
    ///
    /// Finds the cached status for a pod by UID, updates the readiness flag for the specified container,
    /// and recalculates the PodReady and ContainersReady conditions based on all container states.
    /// Signals the sync loop for an on-demand sync if successful.
    ///
    /// Does nothing if:
    /// - Pod UID is not found on the rks API server
    /// - Pod status has not been cached yet
    /// - Container does not exist in the cached status
    /// - Container readiness is already set to the requested value
    ///
    /// # Arguments
    /// * `pod_uid` - The UUID of the pod whose container is being updated
    /// * `container_name` - The name of the container within the pod
    /// * `is_ready` - Whether the container is ready (true) or not (false)
    ///
    /// # Errors
    /// Returns an error if fetching the pod from rks or updating status cache fails.
    pub async fn set_container_readiness(
        &self,
        pod_uid: Uuid,
        container_name: &str,
        is_ready: bool,
    ) -> anyhow::Result<()> {
        debug!(
            pod_uid = %pod_uid,
            container_name,
            is_ready,
            "[StatusManager] Setting container readiness"
        );
        let pod = match get_pod_by_uid(&self.client, &pod_uid).await? {
            Some(p) => p,
            None => {
                debug!(
                    pod_uid = %pod_uid,
                    container_name,
                    is_ready,
                    "[StatusManager] Pod not found on rks; skipping container readiness update"
                );
                return Ok(());
            }
        };

        let (is_cached, mut cached_status) = match self.pod_statuses.get(&pod_uid) {
            Some(s) => (true, s.value().clone()),
            None => (false, VersionedPodStatus::default()),
        };

        if !is_cached {
            debug!(
                pod_uid = %pod_uid,
                pod_name = %pod.metadata.name,
                container_name,
                is_ready,
                "[StatusManager] Container readiness changed before pod status was cached"
            );
            return Ok(());
        }

        let resolved_name = resolve_runtime_container_name(&pod.metadata.name, container_name)
            .unwrap_or_else(|| container_name.to_string());

        let container_status = cached_status
            .status
            .container_statuses
            .iter_mut()
            .find(|container_status| container_status.name == resolved_name);

        if container_status.is_none() {
            debug!(
                pod_uid = %pod_uid,
                pod_name = %cached_status.pod_name,
                container_name,
                is_ready,
                "[StatusManager] Container not found in cached status; skipping readiness update"
            );
            return Ok(());
        }

        let container_status = container_status.unwrap();

        if container_status.ready == is_ready {
            debug!(
                pod_uid = %pod_uid,
                pod_name = %cached_status.pod_name,
                container_name,
                is_ready,
                "[StatusManager] Container readiness already up to date; skipping"
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
                cached_status.status.phase,
            ),
        );

        update_condition(
            PodConditionType::PodReady,
            &create_pod_ready_condition(
                &pod,
                &cached_status.status.container_statuses,
                cached_status.status.phase,
            ),
        );

        self.update_status_internal(&pod, &cached_status.status, false, false)
            .await?;
        debug!(
            pod_uid = %pod_uid,
            pod_name = %pod.metadata.name,
            container_name,
            is_ready,
            "[StatusManager] Container readiness update persisted"
        );

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
        debug!(
            pod_uid = %pod_uid,
            pod_name = %pod.metadata.name,
            pod_namespace = %pod.metadata.namespace,
            incoming_phase = ?status.phase,
            incoming_container_status_count = status.container_statuses.len(),
            force_update,
            pod_is_finished,
            "[StatusManager] update_status_internal start"
        );

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
                "[StatusManager] Illegal container status transition detected for pod '{}': {e}",
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

        if is_cached && is_status_owned_by_rkl_equal(&old_status, &status) && !force_update {
            debug!(
                pod_uid = %pod_uid,
                pod_name = %pod.metadata.name,
                cached_version = cached_status.version,
                "[StatusManager] Pod status unchanged; skipping cache update"
            );

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

        debug!(
            pod_uid = %pod_uid,
            pod_name = %pod.metadata.name,
            version = new_status.version,
            phase = ?new_status.status.phase,
            container_status_count = new_status.status.container_statuses.len(),
            "[StatusManager] Pod status cached with new version"
        );

        // Update the status in the cache.
        self.pod_statuses.insert(pod_uid, new_status);

        // Notify the main loop to process the updated status.
        self.pod_status_update_signal.notify_one();
        debug!(
            pod_uid = %pod_uid,
            pod_name = %pod.metadata.name,
            "[StatusManager] Notified sync loop about cached status update"
        );
        Ok(())
    }
}

impl Drop for StatusManager {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn sync_batch(state: &Arc<State>, sync_all: bool) -> anyhow::Result<()> {
    debug!(
        sync_all,
        cached_pod_count = state.pod_statuses.len(),
        api_version_count = state.api_status_versions.len(),
        "[StatusManager] sync_batch start"
    );
    let mut updated_status: Vec<(Uuid, VersionedPodStatus)> = Vec::new();

    // Clean up orphaned versions.
    if sync_all {
        let mut removed_orphans = 0usize;
        for entry in state.api_status_versions.iter() {
            let uid = *entry.key();
            let has_pod = state.pod_statuses.get(&uid).is_some();
            if !has_pod {
                state.api_status_versions.remove(&uid);
                removed_orphans += 1;
            }
        }
        debug!(
            removed_orphans,
            "[StatusManager] Removed orphaned API status versions during full sync"
        );
    }

    // Decide which pods need status updates.
    for entry in state.pod_statuses.iter() {
        let pod_uid = *entry.key();
        let pod_status = entry.value().clone();

        if !sync_all {
            if let Some(api_version) = state.api_status_versions.get(&pod_uid)
                && *api_version.value() >= pod_status.version
            {
                debug!(
                    pod_uid = %pod_uid,
                    pod_name = %pod_status.pod_name,
                    local_version = pod_status.version,
                    api_version = *api_version.value(),
                    "[StatusManager] Pod status already synced; skipping"
                );
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

    debug!(
        sync_all,
        update_count = updated_status.len(),
        "[StatusManager] sync_batch selected pods for upload"
    );
    for (pod_uid, pod_status) in updated_status {
        debug!(
            pod_uid = %pod_uid,
            pod_name = %pod_status.pod_name,
            version = pod_status.version,
            "[StatusManager] Syncing pod status"
        );
        sync_pod(state, pod_uid, &pod_status).await?;
    }

    debug!(sync_all, "[StatusManager] sync_batch completed");
    Ok(())
}

async fn sync_pod(
    state: &State,
    pod_uid: Uuid,
    pod_status: &VersionedPodStatus,
) -> anyhow::Result<()> {
    debug!(
        pod_uid = %pod_uid,
        pod_name = %pod_status.pod_name,
        pod_namespace = %pod_status.pod_namespace,
        version = pod_status.version,
        "[StatusManager] sync_pod start"
    );
    let pod = match get_pod_by_uid(&state.client, &pod_uid).await? {
        Some(p) => p,
        None => {
            debug!(
                pod_uid = %pod_uid,
                pod_name = %pod_status.pod_name,
                version = pod_status.version,
                "[StatusManager] Pod not found on server; skipping status sync"
            );
            return Ok(());
        }
    };

    let merged_status = merge_status(&pod.status, &pod_status.status).await;
    debug!(
        pod_uid = %pod_uid,
        pod_name = %pod.metadata.name,
        pod_namespace = %pod.metadata.namespace,
        version = pod_status.version,
        merged_phase = ?merged_status.phase,
        "[StatusManager] Merged local and remote pod status; uploading"
    );

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
    debug!(
        pod_uid = %pod_uid,
        pod_name = %pod.metadata.name,
        version = pod_status.version,
        "[StatusManager] sync_pod finished; recorded API status version"
    );

    Ok(())
}

async fn update_pod_status(
    client: &QUICClient<Cli>,
    pod_name: &str,
    pod_namespace: &str,
    pod_status: &PodStatus,
) -> anyhow::Result<()> {
    debug!(
        pod_name,
        pod_namespace,
        phase = ?pod_status.phase,
        container_status_count = pod_status.container_statuses.len(),
        "[StatusManager] Sending UpdatePodStatus request"
    );
    client
        .send_msg(&RksMessage::UpdatePodStatus {
            pod_name: pod_name.to_string(),
            pod_namespace: pod_namespace.to_string(),
            status: pod_status.clone(),
        })
        .await?;

    match client.fetch_msg().await? {
        RksMessage::Ack => {
            debug!(
                pod_name,
                pod_namespace, "[StatusManager] UpdatePodStatus acknowledged"
            );
            Ok(())
        }
        RksMessage::Error(err_msg) => Err(anyhow::anyhow!(
            "[StatusManager] Failed to upload pod status for '{}': {}",
            pod_name,
            err_msg
        )),
        _ => Err(anyhow::anyhow!(
            "[StatusManager] Unexpected response when uploading pod status for '{}'",
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
    if is_pod_phase_terminal(new_pod_status.phase)
        && (get_pod_ready_condition(new_pod_status).is_some()
            || get_container_ready_condition(new_pod_status).is_some())
    {
        let ready_condition = PodCondition {
            condition_type: PodConditionType::PodReady,
            status: common::ConditionStatus::False,
            reason: Some(match new_pod_status.phase {
                PodPhase::Succeeded => "PodCompleted".to_string(),
                PodPhase::Failed => "PodFailed".to_string(),
                _ => "Unknown".to_string(),
            }),
            ..Default::default()
        };

        update_pod_condition(&mut merged_status, ready_condition);

        let containers_ready_condition = PodCondition {
            condition_type: PodConditionType::ContainersReady,
            status: common::ConditionStatus::False,
            reason: Some(match new_pod_status.phase {
                PodPhase::Succeeded => "PodCompleted".to_string(),
                PodPhase::Failed => "PodFailed".to_string(),
                _ => "Unknown".to_string(),
            }),
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
        None => {
            debug!(
                pod_uid = %pod_uid,
                pod_name = %pod_status.pod_name,
                local_version = pod_status.version,
                "[StatusManager] No API version cached; pod status needs upload"
            );
            return Ok(true);
        }
    };

    if latest_api_version < pod_status.version {
        debug!(
            pod_uid = %pod_uid,
            pod_name = %pod_status.pod_name,
            local_version = pod_status.version,
            api_version = latest_api_version,
            "[StatusManager] Local status version is newer than API version"
        );
        return Ok(true);
    }

    let pod = match get_pod_by_uid(&state.client, pod_uid).await? {
        Some(p) => p,
        None => {
            debug!(
                pod_uid = %pod_uid,
                pod_name = %pod_status.pod_name,
                "[StatusManager] Pod not found on server while checking need_update"
            );
            return Ok(false);
        }
    };

    can_be_deleted(pod_status, &pod)
}

fn can_be_deleted(local_status: &VersionedPodStatus, remote_pod: &PodTask) -> anyhow::Result<bool> {
    if remote_pod.metadata.deletion_timestamp.is_none() {
        return Ok(false);
    }

    if !is_pod_phase_terminal(remote_pod.status.phase) {
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

    debug!(
        pod_uid = %pod.metadata.uid,
        pod_name = %pod.metadata.name,
        local_version = pod_status.version,
        local_phase = ?pod_status.status.phase,
        remote_phase = ?pod.status.phase,
        "[StatusManager] Pod status mismatch detected; reconciliation required"
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
        let Some(ContainerState::Terminated { exit_code, .. }) = old_status.state else {
            continue;
        };

        if exit_code != 0 && pod_spec.restart_policy == RestartPolicy::OnFailure {
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

fn is_pod_phase_terminal(phase: PodPhase) -> bool {
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

#[allow(unused)]
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

fn resolve_runtime_container_name(pod_name: &str, container_name: &str) -> Option<String> {
    let root_path = rootpath::determine(None, &*create_syscall()).ok()?;
    let pod_info = PodInfo::load(&root_path, pod_name).ok()?;
    match_container_name(container_name, &pod_info.container_names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_container_spec(name: &str) -> common::ContainerSpec {
        common::ContainerSpec {
            name: name.to_string(),
            image: "image".to_string(),
            ports: Vec::new(),
            args: Vec::new(),
            resources: None,
            liveness_probe: None,
            readiness_probe: None,
            startup_probe: None,
            security_context: None,
            env: None,
            volume_mounts: None,
            command: None,
            working_dir: None,
        }
    }

    fn make_pod_task(container_names: &[&str], restart_policy: RestartPolicy) -> PodTask {
        PodTask {
            api_version: "v1".to_string(),
            kind: "Pod".to_string(),
            metadata: common::ObjectMeta {
                name: "pod".to_string(),
                namespace: "default".to_string(),
                ..Default::default()
            },
            spec: PodSpec {
                node_name: None,
                containers: container_names
                    .iter()
                    .map(|name| make_container_spec(name))
                    .collect(),
                init_containers: Vec::new(),
                tolerations: Vec::new(),
                affinity: None,
                restart_policy,
            },
            status: PodStatus::default(),
        }
    }

    fn make_container_status(
        name: &str,
        ready: bool,
        state: Option<ContainerState>,
    ) -> ContainerStatus {
        ContainerStatus {
            name: name.to_string(),
            ready,
            state,
            ..Default::default()
        }
    }

    #[test]
    fn check_container_status_transition_allows_always_restart() {
        let pod_spec = make_pod_task(&[], RestartPolicy::Always).spec;
        let old_status = PodStatus {
            container_statuses: vec![make_container_status(
                "app",
                false,
                Some(ContainerState::Terminated {
                    exit_code: 0,
                    signal: None,
                    reason: None,
                    message: None,
                    started_at: None,
                    finished_at: None,
                }),
            )],
            ..Default::default()
        };
        let new_status = PodStatus {
            container_statuses: vec![make_container_status(
                "app",
                true,
                Some(ContainerState::Running { started_at: None }),
            )],
            ..Default::default()
        };

        assert!(check_container_status_transition(&old_status, &new_status, &pod_spec).is_ok());
    }

    #[test]
    fn check_container_status_transition_allows_on_failure_nonzero_exit() {
        let pod_spec = make_pod_task(&[], RestartPolicy::OnFailure).spec;
        let old_status = PodStatus {
            container_statuses: vec![make_container_status(
                "app",
                false,
                Some(ContainerState::Terminated {
                    exit_code: 2,
                    signal: None,
                    reason: None,
                    message: None,
                    started_at: None,
                    finished_at: None,
                }),
            )],
            ..Default::default()
        };
        let new_status = PodStatus {
            container_statuses: vec![make_container_status(
                "app",
                true,
                Some(ContainerState::Running { started_at: None }),
            )],
            ..Default::default()
        };

        assert!(check_container_status_transition(&old_status, &new_status, &pod_spec).is_ok());
    }

    #[test]
    fn check_container_status_transition_blocks_restart_on_success() {
        let pod_spec = make_pod_task(&[], RestartPolicy::OnFailure).spec;
        let old_status = PodStatus {
            container_statuses: vec![make_container_status(
                "app",
                false,
                Some(ContainerState::Terminated {
                    exit_code: 0,
                    signal: None,
                    reason: None,
                    message: None,
                    started_at: None,
                    finished_at: None,
                }),
            )],
            ..Default::default()
        };
        let new_status = PodStatus {
            container_statuses: vec![make_container_status(
                "app",
                true,
                Some(ContainerState::Running { started_at: None }),
            )],
            ..Default::default()
        };

        assert!(check_container_status_transition(&old_status, &new_status, &pod_spec).is_err());
    }

    #[test]
    fn check_container_status_transition_blocks_restart_on_never() {
        let pod_spec = make_pod_task(&[], RestartPolicy::Never).spec;
        let old_status = PodStatus {
            container_statuses: vec![make_container_status(
                "app",
                false,
                Some(ContainerState::Terminated {
                    exit_code: 0,
                    signal: None,
                    reason: None,
                    message: None,
                    started_at: None,
                    finished_at: None,
                }),
            )],
            ..Default::default()
        };
        let new_status = PodStatus {
            container_statuses: vec![make_container_status(
                "app",
                true,
                Some(ContainerState::Running { started_at: None }),
            )],
            ..Default::default()
        };

        assert!(check_container_status_transition(&old_status, &new_status, &pod_spec).is_err());
    }

    #[test]
    fn containers_ready_condition_reports_all_ready() {
        let pod = make_pod_task(&["app", "sidecar"], RestartPolicy::Never);
        let statuses = vec![
            make_container_status("app", true, None),
            make_container_status("sidecar", true, None),
        ];

        let condition = create_containers_ready_condition(&pod, &statuses, PodPhase::Running);
        assert_eq!(condition.status, ConditionStatus::True);
        assert!(condition.reason.is_none());
        assert!(condition.message.is_none());
    }

    #[test]
    fn containers_ready_condition_reports_unknown_containers() {
        let pod = make_pod_task(&["app", "sidecar"], RestartPolicy::Never);
        let statuses = vec![make_container_status("app", true, None)];

        let condition = create_containers_ready_condition(&pod, &statuses, PodPhase::Running);
        assert_eq!(condition.status, ConditionStatus::False);
        assert_eq!(condition.reason.as_deref(), Some("ContainersNotReady"));
        assert_eq!(
            condition.message.as_deref(),
            Some("containers with unknown status: [sidecar]")
        );
    }

    #[test]
    fn containers_ready_condition_reports_unready_containers() {
        let pod = make_pod_task(&["app"], RestartPolicy::Never);
        let statuses = vec![make_container_status("app", false, None)];

        let condition = create_containers_ready_condition(&pod, &statuses, PodPhase::Running);
        assert_eq!(condition.status, ConditionStatus::False);
        assert_eq!(condition.reason.as_deref(), Some("ContainersNotReady"));
        assert_eq!(
            condition.message.as_deref(),
            Some("containers with unready status: [app]")
        );
    }

    #[test]
    fn containers_ready_condition_marks_pod_completed_on_success() {
        let pod = make_pod_task(&["app"], RestartPolicy::Never);
        let statuses = vec![make_container_status("app", false, None)];

        let condition = create_containers_ready_condition(&pod, &statuses, PodPhase::Succeeded);
        assert_eq!(condition.status, ConditionStatus::False);
        assert_eq!(condition.reason.as_deref(), Some("PodCompleted"));
    }

    #[test]
    fn pod_ready_condition_bubbles_container_failure_reason() {
        let pod = make_pod_task(&["app"], RestartPolicy::Never);
        let statuses = vec![make_container_status("app", false, None)];

        let condition = create_pod_ready_condition(&pod, &statuses, PodPhase::Running);
        assert_eq!(condition.status, ConditionStatus::False);
        assert_eq!(condition.reason.as_deref(), Some("ContainersNotReady"));
        assert_eq!(
            condition.message.as_deref(),
            Some("containers with unready status: [app]")
        );
    }

    #[test]
    fn update_pod_condition_adds_condition_and_sets_transition_time() {
        let mut status = PodStatus::default();
        let condition = PodCondition {
            condition_type: PodConditionType::PodReady,
            status: ConditionStatus::True,
            ..Default::default()
        };

        assert!(update_pod_condition(&mut status, condition));
        let condition = get_pod_condition(&status, &PodConditionType::PodReady)
            .unwrap()
            .1;
        assert_eq!(condition.status, ConditionStatus::True);
        assert!(condition.last_transition_time.is_some());
    }

    #[test]
    fn update_pod_condition_no_change_preserves_transition_time() {
        let fixed_time = DateTime::<Utc>::from_timestamp_millis(1000).unwrap();
        let mut status = PodStatus {
            conditions: Some(vec![PodCondition {
                condition_type: PodConditionType::PodReady,
                status: ConditionStatus::True,
                last_transition_time: Some(fixed_time),
                reason: Some("old".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let condition = PodCondition {
            condition_type: PodConditionType::PodReady,
            status: ConditionStatus::True,
            reason: Some("new".to_string()),
            ..Default::default()
        };

        assert!(!update_pod_condition(&mut status, condition));
        let condition = get_pod_condition(&status, &PodConditionType::PodReady)
            .unwrap()
            .1;
        assert_eq!(condition.last_transition_time, Some(fixed_time));
        assert_eq!(condition.reason.as_deref(), Some("old"));
    }

    #[test]
    fn update_pod_condition_updates_transition_time_on_status_change() {
        let fixed_time = DateTime::<Utc>::from_timestamp_millis(1000).unwrap();
        let mut status = PodStatus {
            conditions: Some(vec![PodCondition {
                condition_type: PodConditionType::PodReady,
                status: ConditionStatus::True,
                last_transition_time: Some(fixed_time),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let condition = PodCondition {
            condition_type: PodConditionType::PodReady,
            status: ConditionStatus::False,
            reason: Some("flipped".to_string()),
            ..Default::default()
        };

        assert!(update_pod_condition(&mut status, condition));
        let condition = get_pod_condition(&status, &PodConditionType::PodReady)
            .unwrap()
            .1;
        assert_eq!(condition.status, ConditionStatus::False);
        assert_eq!(condition.reason.as_deref(), Some("flipped"));
        assert!(condition.last_transition_time.is_some());
        assert_ne!(condition.last_transition_time, Some(fixed_time));
    }

    #[test]
    fn update_last_transition_time_reuses_timestamp_when_status_unchanged() {
        let fixed_time = DateTime::<Utc>::from_timestamp_millis(2000).unwrap();
        let old_status = PodStatus {
            conditions: Some(vec![PodCondition {
                condition_type: PodConditionType::PodReady,
                status: ConditionStatus::True,
                last_transition_time: Some(fixed_time),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let mut new_status = PodStatus {
            conditions: Some(vec![PodCondition {
                condition_type: PodConditionType::PodReady,
                status: ConditionStatus::True,
                ..Default::default()
            }]),
            ..Default::default()
        };

        update_last_transition_time(&old_status, &mut new_status, &PodConditionType::PodReady)
            .unwrap();
        let condition = get_pod_condition(&new_status, &PodConditionType::PodReady)
            .unwrap()
            .1;
        assert_eq!(condition.last_transition_time, Some(fixed_time));
    }

    #[test]
    fn update_last_transition_time_updates_when_status_changes() {
        let fixed_time = DateTime::<Utc>::from_timestamp_millis(3000).unwrap();
        let old_status = PodStatus {
            conditions: Some(vec![PodCondition {
                condition_type: PodConditionType::PodReady,
                status: ConditionStatus::True,
                last_transition_time: Some(fixed_time),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let mut new_status = PodStatus {
            conditions: Some(vec![PodCondition {
                condition_type: PodConditionType::PodReady,
                status: ConditionStatus::False,
                ..Default::default()
            }]),
            ..Default::default()
        };

        update_last_transition_time(&old_status, &mut new_status, &PodConditionType::PodReady)
            .unwrap();
        let condition = get_pod_condition(&new_status, &PodConditionType::PodReady)
            .unwrap()
            .1;
        assert!(condition.last_transition_time.is_some());
        assert_ne!(condition.last_transition_time, Some(fixed_time));
    }

    #[test]
    fn is_status_owned_by_rkl_equal_ignores_condition_order() {
        let condition_a = PodCondition {
            condition_type: PodConditionType::PodReady,
            status: ConditionStatus::True,
            ..Default::default()
        };
        let condition_b = PodCondition {
            condition_type: PodConditionType::PodInitialized,
            status: ConditionStatus::True,
            ..Default::default()
        };
        let status_a = PodStatus {
            conditions: Some(vec![condition_a.clone(), condition_b.clone()]),
            phase: PodPhase::Running,
            ..Default::default()
        };
        let status_b = PodStatus {
            conditions: Some(vec![condition_b, condition_a]),
            phase: PodPhase::Running,
            ..Default::default()
        };

        assert!(is_status_owned_by_rkl_equal(&status_a, &status_b));
    }

    #[test]
    fn is_status_owned_by_rkl_equal_detects_condition_changes() {
        let status_a = PodStatus {
            conditions: Some(vec![PodCondition {
                condition_type: PodConditionType::PodReady,
                status: ConditionStatus::True,
                reason: Some("ready".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let status_b = PodStatus {
            conditions: Some(vec![PodCondition {
                condition_type: PodConditionType::PodReady,
                status: ConditionStatus::True,
                reason: Some("changed".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        };

        assert!(!is_status_owned_by_rkl_equal(&status_a, &status_b));
    }

    #[test]
    fn is_status_owned_by_rkl_equal_detects_field_changes() {
        let condition = PodCondition {
            condition_type: PodConditionType::PodReady,
            status: ConditionStatus::True,
            ..Default::default()
        };
        let status_a = PodStatus {
            conditions: Some(vec![condition.clone()]),
            phase: PodPhase::Running,
            ..Default::default()
        };
        let status_b = PodStatus {
            conditions: Some(vec![condition]),
            phase: PodPhase::Failed,
            ..Default::default()
        };

        assert!(!is_status_owned_by_rkl_equal(&status_a, &status_b));
    }

    #[tokio::test]
    async fn merge_status_sets_ready_conditions_false_on_terminal_phase() {
        let old_status = PodStatus {
            conditions: Some(vec![PodCondition {
                condition_type: PodConditionType::PodScheduled,
                status: ConditionStatus::True,
                ..Default::default()
            }]),
            ..Default::default()
        };
        let new_status = PodStatus {
            phase: PodPhase::Succeeded,
            conditions: Some(vec![
                PodCondition {
                    condition_type: PodConditionType::PodReady,
                    status: ConditionStatus::True,
                    ..Default::default()
                },
                PodCondition {
                    condition_type: PodConditionType::ContainersReady,
                    status: ConditionStatus::True,
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };

        let merged = merge_status(&old_status, &new_status).await;
        let pod_ready = get_pod_ready_condition(&merged).unwrap();
        let containers_ready = get_container_ready_condition(&merged).unwrap();
        assert_eq!(pod_ready.status, ConditionStatus::False);
        assert_eq!(pod_ready.reason.as_deref(), Some("PodCompleted"));
        assert_eq!(containers_ready.status, ConditionStatus::False);
        assert_eq!(containers_ready.reason.as_deref(), Some("PodCompleted"));
    }

    #[test]
    fn can_be_deleted_requires_deletion_timestamp_terminal_phase_and_finish() {
        let local_status = VersionedPodStatus {
            version: 1,
            status: PodStatus {
                phase: PodPhase::Succeeded,
                ..Default::default()
            },
            pod_name: "pod".to_string(),
            pod_namespace: "default".to_string(),
            pod_is_finished: true,
            at: Utc::now(),
        };
        let mut remote_pod = make_pod_task(&["app"], RestartPolicy::Never);
        remote_pod.status.phase = PodPhase::Succeeded;

        assert!(!can_be_deleted(&local_status, &remote_pod).unwrap());

        remote_pod.metadata.deletion_timestamp = Some(Utc::now());
        assert!(can_be_deleted(&local_status, &remote_pod).unwrap());

        let local_status = VersionedPodStatus {
            pod_is_finished: false,
            ..local_status
        };
        assert!(!can_be_deleted(&local_status, &remote_pod).unwrap());
    }
}
