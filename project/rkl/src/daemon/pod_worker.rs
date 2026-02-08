//! Pod worker event loop that synchronizes pod lifecycle and probe results into container lifecycle actions.
//!
//! [`PodWorker`] is the central event loop that reacts to pod lifecycle events from
//! [`crate::daemon::status::pleg::PLEG`]
//! and probe results from the probe subsystem. On lifecycle events (container creating, started, died),
//! it updates the pod's [`PodStatus`] via the [`StatusManager`]. On probe results, it updates
//! container readiness (readiness probes) or triggers container restarts (liveness probe failures,
//! respecting [`common::RestartPolicy`]). It runs as a single background task consuming from multiple async channels.

use std::sync::Arc;

use chrono::DateTime;
use common::{
    ConditionStatus, ContainerState, ContainerStatus, PodCondition, PodConditionType, PodPhase,
    PodStatus, PodTask,
};
use libcontainer::container::Container;
use libcontainer::syscall::syscall::create_syscall;
use liboci_cli::Delete;
use libruntime::{cri::cri_api::StartContainerRequest, rootpath};
use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use procfs::process::Process;
use tokio::{select, sync::mpsc::UnboundedReceiver, task::JoinHandle};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    commands::{
        delete, load_container,
        pod::{PodInfo, TLSConnectionArgs},
    },
    daemon::status::{
        get_pod_by_uid,
        pleg::{PodLifecycleEvent, PodLifecycleEventType},
        probe::{
            probe_manager::{ProbeManager, ProbeResult, ProbeResultType},
            prober::match_container_name,
        },
        status_manager::StatusManager,
    },
    quic::client::{Cli, QUICClient},
    task::TaskRunner,
};

/// The central event loop that bridges PLEG events and probe results into status updates and container lifecycle actions.
///
/// [`PodWorker`] consumes pod lifecycle events from the [`crate::daemon::status::pleg::PLEG`] and probe results from
/// the probe subsystem. It updates pod status via [`StatusManager`] and triggers container
/// restarts when liveness probes fail (respecting [`common::RestartPolicy`]).
pub struct PodWorker {
    server_addr: String,
    tls_cfg: Arc<TLSConnectionArgs>,
    status_manager: Arc<StatusManager>,
    pod_lifecycle_event_rx: Option<UnboundedReceiver<PodLifecycleEvent>>,
    liveness_probe_result_rx: tokio::sync::broadcast::Receiver<ProbeResult>,
    readiness_probe_result_rx: tokio::sync::broadcast::Receiver<ProbeResult>,
    startup_probe_result_rx: tokio::sync::broadcast::Receiver<ProbeResult>,
    sync_loop_handle: Option<JoinHandle<anyhow::Result<()>>>,
    stop_signal_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

const UNKNOWN_EXIT_CODE: i32 = -1;

struct State {
    server_addr: String,
    tls_cfg: Arc<TLSConnectionArgs>,
    status_manager: Arc<StatusManager>,
    pod_lifecycle_event_rx: UnboundedReceiver<PodLifecycleEvent>,
    liveness_probe_result_rx: tokio::sync::broadcast::Receiver<ProbeResult>,
    readiness_probe_result_rx: tokio::sync::broadcast::Receiver<ProbeResult>,
    startup_probe_result_rx: tokio::sync::broadcast::Receiver<ProbeResult>,
}

impl PodWorker {
    /// Creates a new [`PodWorker`] wired to the given PLEG event receiver, probe manager, and status manager.
    ///
    /// # Arguments
    ///
    /// * `server_addr` - The server address for QUIC connections
    /// * `tls_cfg` - TLS configuration for secure connections
    /// * `pod_lifecycle_event_rx` - Receiver for pod lifecycle events from
    ///   [`crate::daemon::status::pleg::PLEG`]
    /// * `probe_manager` - Manager that provides probe result broadcast channels
    /// * `status_manager` - Manager for updating pod status
    pub fn new(
        server_addr: String,
        tls_cfg: Arc<TLSConnectionArgs>,
        pod_lifecycle_event_rx: UnboundedReceiver<PodLifecycleEvent>,
        probe_manager: Arc<ProbeManager>,
        status_manager: Arc<StatusManager>,
    ) -> Self {
        Self {
            server_addr,
            tls_cfg,
            status_manager,
            pod_lifecycle_event_rx: Some(pod_lifecycle_event_rx),
            liveness_probe_result_rx: probe_manager.liveness_results().updates(),
            readiness_probe_result_rx: probe_manager.readiness_results().updates(),
            startup_probe_result_rx: probe_manager.startup_results().updates(),
            sync_loop_handle: None,
            stop_signal_tx: None,
        }
    }

    /// Starts the event loop as a background tokio task.
    ///
    /// The task will consume from pod lifecycle event and probe result channels,
    /// updating status and handling container restarts until [`Self::stop`] is called.
    pub fn run(&mut self) {
        if let Some(handle) = &self.sync_loop_handle {
            if !handle.is_finished() {
                warn!("[PodWorker] run() called while already running; ignoring.");
                return;
            }
            self.sync_loop_handle = None;
            self.stop_signal_tx = None;
        }

        let Some(pod_lifecycle_event_rx) = self.pod_lifecycle_event_rx.take() else {
            warn!(
                "[PodWorker] run() called after event receiver was consumed; PodWorker cannot be restarted."
            );
            return;
        };

        let (stop_signal_tx, mut stop_signal_rx) = tokio::sync::oneshot::channel();
        self.stop_signal_tx = Some(stop_signal_tx);
        debug!("[PodWorker] Starting event loop task");

        let mut state = State {
            server_addr: self.server_addr.clone(),
            tls_cfg: self.tls_cfg.clone(),
            status_manager: self.status_manager.clone(),
            pod_lifecycle_event_rx,
            liveness_probe_result_rx: self.liveness_probe_result_rx.resubscribe(),
            readiness_probe_result_rx: self.readiness_probe_result_rx.resubscribe(),
            startup_probe_result_rx: self.startup_probe_result_rx.resubscribe(),
        };

        self.sync_loop_handle = Some(tokio::spawn(async move {
            loop {
                select! {
                    Some(event) = state.pod_lifecycle_event_rx.recv() => {
                        tracing::debug!(
                            pod_uid = %event.pod_uid,
                            pod_name = %event.pod_name,
                            event_type = ?event.event_type,
                            container_id = %event.container.state.id,
                            "[PodWorker] Received pod lifecycle event"
                        );
                        if let Err(e) = sync_pod_for_pod_lifecycle_event(&state, &event).await {
                            tracing::error!("Error syncing pod for lifecycle event {:?}: {:?}", event, e);
                        }
                    }
                    Ok(probe_result) = state.liveness_probe_result_rx.recv() => {
                        if let Err(e) = handle_liveness_probe_result(&state, probe_result).await {
                            tracing::error!("Error handling liveness probe result: {e}");
                        }
                    }
                    Ok(probe_result) = state.readiness_probe_result_rx.recv() => {
                        if let Err(e) = handle_readiness_probe_result(&state, probe_result).await {
                            tracing::error!("Error handling readiness probe result: {e}");
                        }
                    }
                    Ok(probe_result) = state.startup_probe_result_rx.recv() => {
                        tracing::debug!(?probe_result, "[PodWorker] Received startup probe result");
                        //TODO: handle startup probe result
                    }
                    _ = &mut stop_signal_rx => {
                        tracing::debug!("[PodWorker] Received stop signal, exiting event loop");
                        break;
                    }
                }
            }
            Ok(())
        }));
    }

    /// Sends a stop signal to the event loop.
    ///
    /// This gracefully shuts down the background task spawned by [`Self::run`].
    pub fn stop(&mut self) {
        if let Some(stop_signal_tx) = self.stop_signal_tx.take() {
            let _ = stop_signal_tx.send(());
        }
        if let Some(sync_loop_handle) = self.sync_loop_handle.take() {
            sync_loop_handle.abort();
        }
    }
}

impl Drop for PodWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn handle_readiness_probe_result(
    state: &State,
    probe_result: ProbeResult,
) -> anyhow::Result<()> {
    tracing::debug!(
        pod_id = %probe_result.pod_id,
        container_id = %probe_result.container_id,
        result = ?probe_result.result,
        "[PodWorker] Handling readiness probe result"
    );
    let is_ready = match probe_result.result {
        ProbeResultType::Success => true,
        ProbeResultType::Failure => false,
        ProbeResultType::Unknown => {
            tracing::debug!(
                pod_id = %probe_result.pod_id,
                container_id = %probe_result.container_id,
                "[PodWorker] Ignoring unknown readiness probe result"
            );
            return Ok(());
        }
    };

    let pod_uid = match Uuid::parse_str(&probe_result.pod_id) {
        Ok(uid) => uid,
        Err(e) => {
            tracing::warn!(
                pod_id = %probe_result.pod_id,
                error = %e,
                "[PodWorker] Invalid pod uid in readiness probe result"
            );
            return Ok(());
        }
    };

    tracing::debug!(
        pod_uid = %pod_uid,
        container_id = %probe_result.container_id,
        is_ready,
        "[PodWorker] Updating container readiness in status manager"
    );
    state
        .status_manager
        .set_container_readiness(pod_uid, &probe_result.container_id, is_ready)
        .await?;
    tracing::debug!(
        pod_uid = %pod_uid,
        container_id = %probe_result.container_id,
        is_ready,
        "[PodWorker] Updated container readiness in status manager"
    );

    Ok(())
}

async fn handle_liveness_probe_result(
    state: &State,
    probe_result: ProbeResult,
) -> anyhow::Result<()> {
    tracing::debug!(
        pod_id = %probe_result.pod_id,
        container_id = %probe_result.container_id,
        result = ?probe_result.result,
        "[PodWorker] Handling liveness probe result"
    );
    match probe_result.result {
        ProbeResultType::Success => {
            tracing::debug!(
                pod_id = %probe_result.pod_id,
                container_id = %probe_result.container_id,
                "[PodWorker] Liveness probe succeeded"
            );
            return Ok(());
        }
        ProbeResultType::Unknown => {
            tracing::debug!(
                pod_id = %probe_result.pod_id,
                container_id = %probe_result.container_id,
                "[PodWorker] Ignoring unknown liveness probe result"
            );
            return Ok(());
        }
        ProbeResultType::Failure => {}
    }

    let pod_uid = match Uuid::parse_str(&probe_result.pod_id) {
        Ok(uid) => uid,
        Err(e) => {
            tracing::warn!(
                pod_id = %probe_result.pod_id,
                error = %e,
                "[PodWorker] Invalid pod uid in liveness probe result"
            );
            return Ok(());
        }
    };

    tracing::debug!(
        pod_uid = %pod_uid,
        container_id = %probe_result.container_id,
        "[PodWorker] Querying pod for liveness failure handling"
    );
    let client = QUICClient::<Cli>::connect(&state.server_addr, &state.tls_cfg).await?;
    let pod = match get_pod_by_uid(&client, &pod_uid).await? {
        Some(pod) => pod,
        None => {
            tracing::debug!(
                pod_id = %probe_result.pod_id,
                "[PodWorker] Pod not found for liveness probe result"
            );
            return Ok(());
        }
    };

    match pod.spec.restart_policy {
        common::RestartPolicy::Never => {
            tracing::debug!(
                pod_name = %pod.metadata.name,
                container_id = %probe_result.container_id,
                "[PodWorker] Skipping restart for liveness failure due to RestartPolicy::Never"
            );
            return Ok(());
        }
        common::RestartPolicy::Always | common::RestartPolicy::OnFailure => {}
    }

    let root_path = rootpath::determine(None, &*create_syscall())?;
    let resolved_container_id = PodInfo::load(&root_path, &pod.metadata.name)
        .ok()
        .and_then(|info| match_container_name(&probe_result.container_id, &info.container_names))
        .unwrap_or_else(|| probe_result.container_id.clone());

    let mut container = Container::default();
    container.state.id = resolved_container_id;

    let event = PodLifecycleEvent {
        pod_uid,
        pod_name: pod.metadata.name.clone(),
        event_type: PodLifecycleEventType::ContainerDied,
        container,
    };

    tracing::info!(
        pod_name = %pod.metadata.name,
        pod_uid = %pod_uid,
        container_id = %probe_result.container_id,
        "[PodWorker] Restarting container due to liveness probe failure"
    );
    restart_container_locally(&pod, &event).await?;
    tracing::debug!(
        pod_name = %pod.metadata.name,
        pod_uid = %pod_uid,
        container_id = %probe_result.container_id,
        "[PodWorker] Restart finished for liveness probe failure"
    );
    Ok(())
}

async fn sync_pod_for_pod_lifecycle_event(
    state: &State,
    event: &PodLifecycleEvent,
) -> anyhow::Result<()> {
    let status_manager = state.status_manager.clone();

    debug!(
        pod_uid = %event.pod_uid,
        pod_name = %event.pod_name,
        event_type = ?event.event_type,
        container_id = %event.container.state.id,
        "[PodWorker] Syncing pod for lifecycle event"
    );

    let client = QUICClient::<Cli>::connect(&state.server_addr, &state.tls_cfg).await?;
    let pod = get_pod_by_uid(&client, &event.pod_uid)
        .await?
        .ok_or(anyhow::anyhow!("Pod not found"))?;

    let mut pod_status = match status_manager.get_pod_status(event.pod_uid).await {
        Some(status) => status,
        None => {
            debug!(
                pod_uid = %event.pod_uid,
                pod_name = %event.pod_name,
                "[PodWorker] Pod status not cached; initializing from API status"
            );
            pod.status.clone()
        }
    };

    debug!(
        pod_uid = %event.pod_uid,
        pod_name = %event.pod_name,
        status = ?pod_status,
        "[PodWorker] Current cached pod status before lifecycle handling"
    );

    apply_pod_lifecycle_event(&pod, &mut pod_status, event).await?;

    debug!(
        pod_uid = %event.pod_uid,
        pod_name = %event.pod_name,
        status = ?pod_status,
        "[PodWorker] Pod status after lifecycle handling"
    );

    status_manager.set_pod_status(&pod, &pod_status).await?;
    debug!(
        pod_uid = %event.pod_uid,
        pod_name = %event.pod_name,
        "[PodWorker] Persisted lifecycle-derived pod status to status manager"
    );
    Ok(())
}

async fn apply_pod_lifecycle_event(
    pod_task: &PodTask,
    pod_status: &mut PodStatus,
    event: &PodLifecycleEvent,
) -> anyhow::Result<()> {
    if pod_status.conditions.is_none() {
        pod_status.conditions = Some(Vec::new());
    }

    let container = &event.container;
    match event.event_type {
        PodLifecycleEventType::ContainerCreating => {
            debug!(
                pod_uid = %event.pod_uid,
                pod_name = %event.pod_name,
                container_id = %container.state.id,
                "[PodWorker] Handling ContainerCreating event"
            );

            pod_status.phase = PodPhase::Pending;
            let pod_conditions = pod_status.conditions.as_mut().unwrap();
            match pod_conditions
                .iter_mut()
                .find(|cond| cond.condition_type == PodConditionType::PodScheduled)
            {
                Some(condition) => {
                    condition.status = ConditionStatus::True;
                }
                None => {
                    let condition = PodCondition {
                        condition_type: PodConditionType::PodScheduled,
                        status: ConditionStatus::True,
                        ..Default::default()
                    };
                    pod_conditions.push(condition);
                }
            }
            match pod_status
                .container_statuses
                .iter_mut()
                .find(|cs| cs.name == container.state.id)
            {
                Some(container_status) => {
                    container_status.state = Some(ContainerState::Waiting {
                        reason: Some("Pulling".to_string()),
                        message: None,
                    });
                }
                None => {
                    let container_status = ContainerStatus {
                        name: container.state.id.clone(),
                        state: Some(ContainerState::Waiting {
                            reason: Some("Pulling".to_string()),
                            message: None,
                        }),
                        ..Default::default()
                    };
                    pod_status.container_statuses.push(container_status);
                }
            };
        }
        PodLifecycleEventType::ContainerStarted => {
            debug!(
                pod_uid = %event.pod_uid,
                pod_name = %event.pod_name,
                container_id = %container.state.id,
                "[PodWorker] Handling ContainerStarted event"
            );

            pod_status.phase = PodPhase::Running;
            match pod_status
                .container_statuses
                .iter_mut()
                .find(|cs| cs.name == container.state.id)
            {
                Some(container_status) => {
                    container_status.name = container.state.id.clone();
                    container_status.state = Some(ContainerState::Running {
                        started_at: container.state.created,
                    });
                }
                None => {
                    let container_status = ContainerStatus {
                        name: container.state.id.clone(),
                        state: Some(ContainerState::Running {
                            started_at: container.state.created,
                        }),
                        ..Default::default()
                    };
                    pod_status.container_statuses.push(container_status);
                }
            };
        }
        PodLifecycleEventType::ContainerDied => {
            debug!(
                pod_uid = %event.pod_uid,
                pod_name = %event.pod_name,
                container_id = %container.state.id,
                "[PodWorker] Handling ContainerDied event"
            );

            let (exit_code, signal, message) = resolve_exit_status(&event.container);

            match pod_status
                .container_statuses
                .iter_mut()
                .find(|cs| cs.name == container.state.id)
            {
                Some(container_status) => {
                    container_status.name = container.state.id.clone();
                    container_status.state = Some(ContainerState::Terminated {
                        exit_code,
                        started_at: container.state.created,
                        finished_at: Some(DateTime::<chrono::Utc>::from(
                            std::time::SystemTime::now(),
                        )),
                        signal,
                        reason: Some("ContainerDied".to_string()),
                        message,
                    });
                }
                None => {
                    let container_status = ContainerStatus {
                        name: container.state.id.clone(),
                        state: Some(ContainerState::Terminated {
                            exit_code,
                            started_at: container.state.created,
                            finished_at: Some(DateTime::<chrono::Utc>::from(
                                std::time::SystemTime::now(),
                            )),
                            signal,
                            reason: Some("ContainerDied".to_string()),
                            message,
                        }),
                        ..Default::default()
                    };
                    pod_status.container_statuses.push(container_status);
                }
            };

            match pod_task.spec.restart_policy {
                common::RestartPolicy::Always => {
                    info!(
                        pod_uid = %event.pod_uid,
                        pod_name = %event.pod_name,
                        container_id = %container.state.id,
                        "[PodWorker] Restarting container due to RestartPolicy::Always"
                    );

                    restart_container_locally(pod_task, event).await?;
                }
                common::RestartPolicy::OnFailure => {}
                common::RestartPolicy::Never => {}
            }
        }
        _ => {
            debug!(
                pod_uid = %event.pod_uid,
                pod_name = %event.pod_name,
                event_type = ?event.event_type,
                "[PodWorker] Unhandled PodLifecycleEventType"
            );
        }
    }

    // update pod phase to Succeeded or Failed if all containers are terminated
    if !pod_status.container_statuses.is_empty()
        && pod_status
            .container_statuses
            .iter()
            .all(|cs| matches!(cs.state, Some(ContainerState::Terminated { .. })))
    {
        let has_non_zero_exit = pod_status.container_statuses.iter().any(|cs| {
            matches!(
                cs.state,
                Some(ContainerState::Terminated { exit_code, .. }) if exit_code != 0
            )
        });
        pod_status.phase = if has_non_zero_exit {
            PodPhase::Failed
        } else {
            PodPhase::Succeeded
        };
    }
    debug!(
        pod_uid = %event.pod_uid,
        pod_name = %event.pod_name,
        phase = ?pod_status.phase,
        container_status_count = pod_status.container_statuses.len(),
        "[PodWorker] Finished applying lifecycle event to pod status"
    );
    Ok(())
}

fn resolve_exit_status(container: &Container) -> (i32, Option<i32>, Option<String>) {
    match exit_status_from_container(container) {
        Some((exit_code, signal)) => {
            tracing::debug!(
                container_id = %container.state.id,
                exit_code,
                signal = ?signal,
                "[PodWorker] Resolved container exit status from process state"
            );
            (exit_code, signal, None)
        }
        None => {
            tracing::warn!(
                container_id = %container.state.id,
                "[PodWorker] Exit code unavailable; using fallback value"
            );
            (
                UNKNOWN_EXIT_CODE,
                None,
                Some("exit code unavailable".to_string()),
            )
        }
    }
}

fn exit_status_from_container(container: &Container) -> Option<(i32, Option<i32>)> {
    let pid = match container.state.pid {
        Some(pid) => pid,
        None => {
            tracing::debug!(
                container_id = %container.state.id,
                "[PodWorker] Container pid is unavailable while resolving exit status"
            );
            return None;
        }
    };
    let process = Process::new(pid).ok()?;
    let stat = process.stat().ok()?;
    let raw_status = stat.exit_code?;
    tracing::debug!(
        container_id = %container.state.id,
        pid,
        raw_status,
        "[PodWorker] Decoding raw wait status from /proc"
    );
    decode_wait_status(raw_status)
}

fn decode_wait_status(raw_status: i32) -> Option<(i32, Option<i32>)> {
    let pid = Pid::from_raw(1);
    let status = WaitStatus::from_raw(pid, raw_status).ok()?;
    match status {
        WaitStatus::Exited(_, code) => Some((code, None)),
        WaitStatus::Signaled(_, signal, _) => {
            let signal_code = signal as i32;
            Some((128 + signal_code, Some(signal_code)))
        }
        _ => {
            tracing::debug!(
                raw_status,
                wait_status = ?status,
                "[PodWorker] Unsupported wait status variant while decoding exit status"
            );
            None
        }
    }
}

async fn restart_container_locally(
    pod_task: &PodTask,
    event: &PodLifecycleEvent,
) -> anyhow::Result<()> {
    let container_id = &event.container.state.id;
    tracing::debug!(
        pod_uid = %event.pod_uid,
        pod_name = %event.pod_name,
        container_id = %container_id,
        "[PodWorker] restart_container_locally started"
    );
    let root_path = rootpath::determine(None, &*create_syscall())?;
    let pod_info = PodInfo::load(&root_path, &event.pod_name)?;
    let pod_sandbox = load_container(root_path.clone(), &pod_info.pod_sandbox_id)?;
    let pause_pid = pod_sandbox.state.pid.ok_or(anyhow::anyhow!(
        "Pause container PID not found for pod {} (sandbox id: {})",
        event.pod_name,
        pod_info.pod_sandbox_id
    ))?;

    let mut task_runner = TaskRunner::from_task(pod_task.clone())?;
    task_runner.pause_pid = Some(pause_pid);
    task_runner.sandbox_config =
        Some(task_runner.create_pod_sandbox_config(&event.pod_uid.to_string(), 0)?);
    tracing::debug!(
        pod_uid = %event.pod_uid,
        pod_name = %event.pod_name,
        container_id = %container_id,
        pause_pid,
        sandbox_id = %pod_info.pod_sandbox_id,
        "[PodWorker] Loaded pod sandbox context for container restart"
    );

    let container_spec = task_runner
        .task
        .spec
        .containers
        .iter()
        .find(|c| {
            c.name == *container_id
                || match_container_name(&c.name, &pod_info.container_names).as_deref()
                    == Some(container_id.as_str())
        })
        .ok_or(anyhow::anyhow!(
            "Container spec not found for id {} in pod {}",
            container_id,
            event.pod_name
        ))?;

    if root_path.join(container_id).exists() {
        tracing::debug!(
            pod_uid = %event.pod_uid,
            pod_name = %event.pod_name,
            container_id = %container_id,
            "[PodWorker] Removing existing local container state before restart"
        );
        delete(
            Delete {
                container_id: container_id.clone(),
                force: true,
            },
            root_path.clone(),
        )?;
    }

    let create_request = task_runner
        .build_create_container_request(&pod_info.pod_sandbox_id, container_spec)
        .await?;
    let create_response = task_runner.create_container(create_request)?;
    task_runner.start_container(StartContainerRequest {
        container_id: create_response.container_id,
    })?;
    tracing::debug!(
        pod_uid = %event.pod_uid,
        pod_name = %event.pod_name,
        container_id = %container_id,
        "[PodWorker] restart_container_locally completed"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use common::{ContainerSpec, ObjectMeta, PodSpec};
    use libcontainer::container::{Container, ContainerStatus as LibContainerStatus};
    use uuid::Uuid;

    fn make_container(id: &str, created_at: Option<DateTime<chrono::Utc>>) -> Container {
        let mut container = Container::default();
        container.state.id = id.to_string();
        container.state.status = LibContainerStatus::Running;
        container.state.created = created_at;
        container
    }

    fn make_event(event_type: PodLifecycleEventType, container: Container) -> PodLifecycleEvent {
        PodLifecycleEvent {
            pod_uid: Uuid::nil(),
            pod_name: "pod".to_string(),
            event_type,
            container,
        }
    }

    fn make_pod_task(restart_policy: common::RestartPolicy) -> PodTask {
        PodTask {
            api_version: "v1".to_string(),
            kind: "Pod".to_string(),
            metadata: ObjectMeta {
                name: "pod".to_string(),
                ..Default::default()
            },
            spec: PodSpec {
                node_name: None,
                containers: vec![ContainerSpec {
                    name: "c1".to_string(),
                    image: "bundle".to_string(),
                    ports: vec![],
                    args: vec![],
                    resources: None,
                    liveness_probe: None,
                    readiness_probe: None,
                    startup_probe: None,
                    security_context: None,
                    env: None,
                    volume_mounts: None,
                    command: None,
                    working_dir: None,
                }],
                init_containers: vec![],
                tolerations: vec![],
                affinity: None,
                restart_policy,
            },
            status: PodStatus::default(),
        }
    }

    #[tokio::test]
    async fn apply_event_container_creating_sets_pending_and_condition() {
        let pod_task = make_pod_task(common::RestartPolicy::Never);
        let mut pod_status = PodStatus::default();
        let container = make_container("c1", None);
        let event = make_event(PodLifecycleEventType::ContainerCreating, container);

        apply_pod_lifecycle_event(&pod_task, &mut pod_status, &event)
            .await
            .unwrap();

        assert_eq!(pod_status.phase, PodPhase::Pending);
        let conditions = pod_status.conditions.as_ref().unwrap();
        let scheduled = conditions
            .iter()
            .find(|cond| cond.condition_type == PodConditionType::PodScheduled)
            .unwrap();
        assert_eq!(scheduled.status, ConditionStatus::True);

        let container_status = pod_status
            .container_statuses
            .iter()
            .find(|cs| cs.name == "c1")
            .unwrap();
        match container_status.state.as_ref().unwrap() {
            ContainerState::Waiting { reason, message } => {
                assert_eq!(reason.as_deref(), Some("Pulling"));
                assert!(message.is_none());
            }
            state => panic!("unexpected container state: {state:?}"),
        }
    }

    #[tokio::test]
    async fn apply_event_container_started_sets_running_state() {
        let pod_task = make_pod_task(common::RestartPolicy::Never);
        let created_at = Utc::now();
        let container = make_container("c1", Some(created_at));
        let event = make_event(PodLifecycleEventType::ContainerStarted, container);

        let mut pod_status = PodStatus::default();
        pod_status.container_statuses.push(ContainerStatus {
            name: "c1".to_string(),
            state: Some(ContainerState::Waiting {
                reason: Some("Pulling".to_string()),
                message: None,
            }),
            ..Default::default()
        });

        apply_pod_lifecycle_event(&pod_task, &mut pod_status, &event)
            .await
            .unwrap();

        assert_eq!(pod_status.phase, PodPhase::Running);
        let container_status = pod_status
            .container_statuses
            .iter()
            .find(|cs| cs.name == "c1")
            .unwrap();
        match container_status.state.as_ref().unwrap() {
            ContainerState::Running { started_at } => {
                assert_eq!(*started_at, Some(created_at));
            }
            state => panic!("unexpected container state: {state:?}"),
        }
    }

    #[tokio::test]
    async fn apply_event_container_died_fails_when_any_exit_code_non_zero() {
        let pod_task = make_pod_task(common::RestartPolicy::Never);
        let created_at = Utc::now();
        let container = make_container("c1", Some(created_at));
        let event = make_event(PodLifecycleEventType::ContainerDied, container);

        let mut pod_status = PodStatus::default();
        pod_status.phase = PodPhase::Running;

        apply_pod_lifecycle_event(&pod_task, &mut pod_status, &event)
            .await
            .unwrap();

        assert_eq!(pod_status.phase, PodPhase::Failed);
        let container_status = pod_status
            .container_statuses
            .iter()
            .find(|cs| cs.name == "c1")
            .unwrap();
        match container_status.state.as_ref().unwrap() {
            ContainerState::Terminated {
                exit_code,
                reason,
                started_at,
                finished_at,
                message,
                ..
            } => {
                assert_eq!(*exit_code, UNKNOWN_EXIT_CODE);
                assert_eq!(reason.as_deref(), Some("ContainerDied"));
                assert_eq!(*started_at, Some(created_at));
                assert!(finished_at.is_some());
                assert_eq!(message.as_deref(), Some("exit code unavailable"));
            }
            state => panic!("unexpected container state: {state:?}"),
        }
    }

    #[tokio::test]
    async fn apply_event_all_terminated_with_zero_exit_code_sets_succeeded() {
        let pod_task = make_pod_task(common::RestartPolicy::Never);
        let container = make_container("c1", None);
        let event = make_event(PodLifecycleEventType::ContainerChanged, container);

        let mut pod_status = PodStatus::default();
        pod_status.phase = PodPhase::Running;
        pod_status.container_statuses.push(ContainerStatus {
            name: "c1".to_string(),
            state: Some(ContainerState::Terminated {
                exit_code: 0,
                started_at: None,
                finished_at: None,
                signal: None,
                reason: Some("Completed".to_string()),
                message: None,
            }),
            ..Default::default()
        });

        apply_pod_lifecycle_event(&pod_task, &mut pod_status, &event)
            .await
            .unwrap();

        assert_eq!(pod_status.phase, PodPhase::Succeeded);
    }

    #[tokio::test]
    async fn apply_event_container_died_does_not_complete_with_running_container() {
        let pod_task = make_pod_task(common::RestartPolicy::Never);
        let created_at = Utc::now();
        let container = make_container("c1", Some(created_at));
        let event = make_event(PodLifecycleEventType::ContainerDied, container);

        let mut pod_status = PodStatus::default();
        pod_status.phase = PodPhase::Running;
        pod_status.container_statuses.push(ContainerStatus {
            name: "sidecar".to_string(),
            state: Some(ContainerState::Running { started_at: None }),
            ..Default::default()
        });

        apply_pod_lifecycle_event(&pod_task, &mut pod_status, &event)
            .await
            .unwrap();

        assert_eq!(pod_status.phase, PodPhase::Running);
    }

    #[test]
    fn decode_wait_status_exited() {
        let status = decode_wait_status(0x0200).expect("status");
        assert_eq!(status.0, 2);
        assert!(status.1.is_none());
    }

    #[test]
    fn decode_wait_status_signaled() {
        let status = decode_wait_status(0x0009).expect("status");
        assert_eq!(status.0, 137);
        assert_eq!(status.1, Some(9));
    }
}
