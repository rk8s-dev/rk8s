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
use tokio::{select, sync::mpsc::UnboundedReceiver, task::JoinHandle};
use tracing::info;
use uuid::Uuid;

use crate::{
    commands::{
        delete, load_container,
        pod::{PodInfo, TLSConnectionArgs},
    },
    daemon::status::{
        get_pod_by_uid,
        pleg::{PodLifecycleEvent, PodLifecycleEventType},
        probe::probe_manager::{ProbeManager, ProbeResult, ProbeResultType},
        status_manager::StatusManager,
    },
    quic::client::{Cli, QUICClient},
    task::TaskRunner,
};

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

    pub fn run(&mut self) {
        let (stop_signal_tx, mut stop_signal_rx) = tokio::sync::oneshot::channel();
        self.stop_signal_tx = Some(stop_signal_tx);

        let mut state = State {
            server_addr: self.server_addr.clone(),
            tls_cfg: self.tls_cfg.clone(),
            status_manager: self.status_manager.clone(),
            pod_lifecycle_event_rx: self.pod_lifecycle_event_rx.take().unwrap(),
            liveness_probe_result_rx: self.liveness_probe_result_rx.resubscribe(),
            readiness_probe_result_rx: self.readiness_probe_result_rx.resubscribe(),
            startup_probe_result_rx: self.startup_probe_result_rx.resubscribe(),
        };

        self.sync_loop_handle = Some(tokio::spawn(async move {
            loop {
                select! {
                    Some(event) = state.pod_lifecycle_event_rx.recv() => {
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
                        tracing::info!("Received startup probe result: {:?}", probe_result);
                        //TODO: handle startup probe result
                    }
                    _ = &mut stop_signal_rx => {
                        tracing::info!("PodWorker received stop signal, exiting.");
                        break;
                    }
                }
            }
            Ok(())
        }));
    }

    pub fn stop(&mut self) {
        if let Some(stop_signal_tx) = self.stop_signal_tx.take() {
            let _ = stop_signal_tx.send(());
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
    let is_ready = match probe_result.result {
        ProbeResultType::Success => true,
        ProbeResultType::Failure => false,
        ProbeResultType::Unknown => {
            tracing::info!(
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

    state
        .status_manager
        .set_container_readiness(pod_uid, &probe_result.container_id, is_ready)
        .await?;

    Ok(())
}

async fn handle_liveness_probe_result(
    state: &State,
    probe_result: ProbeResult,
) -> anyhow::Result<()> {
    match probe_result.result {
        ProbeResultType::Success => return Ok(()),
        ProbeResultType::Unknown => {
            tracing::info!(
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

    let client = QUICClient::<Cli>::connect(&state.server_addr, &state.tls_cfg).await?;
    let pod = match get_pod_by_uid(&client, &pod_uid).await? {
        Some(pod) => pod,
        None => {
            tracing::info!(
                pod_id = %probe_result.pod_id,
                "[PodWorker] Pod not found for liveness probe result"
            );
            return Ok(());
        }
    };

    match pod.spec.restart_policy {
        common::RestartPolicy::Never => {
            tracing::info!(
                pod_name = %pod.metadata.name,
                container_id = %probe_result.container_id,
                "[PodWorker] Skipping restart for liveness failure due to RestartPolicy::Never"
            );
            return Ok(());
        }
        common::RestartPolicy::Always | common::RestartPolicy::OnFailure => {}
    }

    let mut container = Container::default();
    container.state.id = probe_result.container_id.clone();

    let event = PodLifecycleEvent {
        pod_uid,
        pod_name: pod.metadata.name.clone(),
        event_type: PodLifecycleEventType::ContainerDied,
        container,
    };

    restart_container_locally(&pod, &event).await?;
    Ok(())
}

async fn sync_pod_for_pod_lifecycle_event(
    state: &State,
    event: &PodLifecycleEvent,
) -> anyhow::Result<()> {
    let status_manager = state.status_manager.clone();

    info!(
        "[PodWorker] Syncing pod {:?} for lifecycle event {:?}",
        event.pod_name, event.event_type
    );

    let client = QUICClient::<Cli>::connect(&state.server_addr, &state.tls_cfg).await?;
    let pod = get_pod_by_uid(&client, &event.pod_uid)
        .await?
        .ok_or(anyhow::anyhow!("Pod not found"))?;

    let mut pod_status = match status_manager.get_pod_status(event.pod_uid).await {
        Some(status) => status,
        None => {
            info!(
                "[PodWorker] Pod status not found for {:?}, initializing from API status",
                event.pod_name
            );
            pod.status.clone()
        }
    };

    info!(
        "[PodWorker] Current pod status for pod {:?}: {:?}",
        event.pod_name, pod_status
    );

    apply_pod_lifecycle_event(&pod, &mut pod_status, event).await?;

    info!(
        "[PodWorker] Updated pod status for pod {:?}: {:?}",
        event.pod_name, pod_status
    );

    status_manager.set_pod_status(&pod, &pod_status).await?;
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
            info!(
                "[PodWorker] Handling ContainerCreating event for pod {:?}",
                event.pod_name
            );

            pod_status.phase = Some(PodPhase::Pending);
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
            info!(
                "[PodWorker] Handling ContainerStarted event for pod {:?}",
                event.pod_name
            );

            pod_status.phase = Some(PodPhase::Running);
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
            info!(
                "[PodWorker] Handling ContainerDied event for pod {:?}",
                event.pod_name
            );

            match pod_status
                .container_statuses
                .iter_mut()
                .find(|cs| cs.name == container.state.id)
            {
                Some(container_status) => {
                    container_status.name = container.state.id.clone();
                    container_status.state = Some(ContainerState::Terminated {
                        // TODO: get real exit code
                        exit_code: 0,
                        started_at: container.state.created,
                        finished_at: Some(DateTime::<chrono::Utc>::from(
                            std::time::SystemTime::now(),
                        )),
                        signal: None,
                        reason: Some("ContainerDied".to_string()),
                        message: None,
                    });
                }
                None => {
                    let container_status = ContainerStatus {
                        name: container.state.id.clone(),
                        state: Some(ContainerState::Terminated {
                            // TODO: get real exit code
                            exit_code: 0,
                            started_at: container.state.created,
                            finished_at: Some(DateTime::<chrono::Utc>::from(
                                std::time::SystemTime::now(),
                            )),
                            signal: None,
                            reason: Some("ContainerDied".to_string()),
                            message: None,
                        }),
                        ..Default::default()
                    };
                    pod_status.container_statuses.push(container_status);
                }
            };

            match pod_task.spec.restart_policy {
                common::RestartPolicy::Always => {
                    info!(
                        "[PodWorker] Restarting container {} in pod {:?} due to RestartPolicy::Always",
                        container.state.id, event.pod_name
                    );

                    restart_container_locally(pod_task, event).await?;
                }
                common::RestartPolicy::OnFailure => {}
                common::RestartPolicy::Never => {}
            }
        }
        _ => {
            info!(
                "[PodWorker] Unhandled PodLifecycleEventType: {:?}",
                event.event_type
            );
        }
    }

    // update pod phase to Succeeded or Failed if all containers are terminated
    // TODO: set Failed phase if any container exited with non-zero code
    if pod_status
        .container_statuses
        .iter()
        .all(|cs| matches!(cs.state, Some(ContainerState::Terminated { .. })))
    {
        pod_status.phase = Some(PodPhase::Succeeded);
    }
    Ok(())
}

async fn restart_container_locally(pod_task: &PodTask, event: &PodLifecycleEvent) -> anyhow::Result<()> {
    let container_id = &event.container.state.id;
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

    let container_spec = task_runner
        .task
        .spec
        .containers
        .iter()
        .find(|c| c.name == *container_id)
        .ok_or(anyhow::anyhow!(
            "Container spec not found for id {} in pod {}",
            container_id,
            event.pod_name
        ))?;

    if root_path.join(container_id).exists() {
        delete(
            Delete {
                container_id: container_id.clone(),
                force: true,
            },
            root_path.clone(),
        )?;
    }

    let create_request =
        task_runner.build_create_container_request(&pod_info.pod_sandbox_id, container_spec).await?;
    let create_response = task_runner.create_container(create_request)?;
    task_runner.start_container(StartContainerRequest {
        container_id: create_response.container_id,
    })?;

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

        assert_eq!(pod_status.phase, Some(PodPhase::Pending));
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

        assert_eq!(pod_status.phase, Some(PodPhase::Running));
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
    async fn apply_event_container_died_succeeds_when_all_terminated() {
        let pod_task = make_pod_task(common::RestartPolicy::Never);
        let created_at = Utc::now();
        let container = make_container("c1", Some(created_at));
        let event = make_event(PodLifecycleEventType::ContainerDied, container);

        let mut pod_status = PodStatus::default();
        pod_status.phase = Some(PodPhase::Running);

        apply_pod_lifecycle_event(&pod_task, &mut pod_status, &event)
            .await
            .unwrap();

        assert_eq!(pod_status.phase, Some(PodPhase::Succeeded));
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
                ..
            } => {
                assert_eq!(*exit_code, 0);
                assert_eq!(reason.as_deref(), Some("ContainerDied"));
                assert_eq!(*started_at, Some(created_at));
                assert!(finished_at.is_some());
            }
            state => panic!("unexpected container state: {state:?}"),
        }
    }

    #[tokio::test]
    async fn apply_event_container_died_does_not_complete_with_running_container() {
        let pod_task = make_pod_task(common::RestartPolicy::Never);
        let created_at = Utc::now();
        let container = make_container("c1", Some(created_at));
        let event = make_event(PodLifecycleEventType::ContainerDied, container);

        let mut pod_status = PodStatus::default();
        pod_status.phase = Some(PodPhase::Running);
        pod_status.container_statuses.push(ContainerStatus {
            name: "sidecar".to_string(),
            state: Some(ContainerState::Running { started_at: None }),
            ..Default::default()
        });

        apply_pod_lifecycle_event(&pod_task, &mut pod_status, &event)
            .await
            .unwrap();

        assert_eq!(pod_status.phase, Some(PodPhase::Running));
    }
}
