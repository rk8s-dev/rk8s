use std::sync::Arc;

use chrono::DateTime;
use common::{
    ConditionStatus, ContainerState, ContainerStatus, PodCondition, PodConditionType, PodPhase,
    PodStatus,
};
use tokio::{select, sync::mpsc::UnboundedReceiver, task::JoinHandle};

use crate::{
    commands::pod::TLSConnectionArgs,
    daemon::status::{
        get_pod_by_uid,
        pleg::{PodLifecycleEvent, PodLifecycleEventType},
        status_manager::STATUS_MANAGER,
    },
    quic::client::{Daemon, QUICClient},
};

pub struct PodWorker {
    server_addr: String,
    tls_cfg: Arc<TLSConnectionArgs>,
    pod_lifecycle_event_rx: Option<UnboundedReceiver<PodLifecycleEvent>>,
    sync_loop_handle: Option<JoinHandle<anyhow::Result<()>>>,
    stop_signal_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Debug)]
struct State {
    server_addr: String,
    tls_cfg: Arc<TLSConnectionArgs>,
    pod_lifecycle_event_rx: UnboundedReceiver<PodLifecycleEvent>,
}

impl PodWorker {
    pub fn new(
        server_addr: String,
        tls_cfg: Arc<TLSConnectionArgs>,
        pod_lifecycle_event_rx: UnboundedReceiver<PodLifecycleEvent>,
    ) -> Self {
        Self {
            server_addr,
            tls_cfg,
            pod_lifecycle_event_rx: Some(pod_lifecycle_event_rx),
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
            pod_lifecycle_event_rx: self.pod_lifecycle_event_rx.take().unwrap(),
        };

        self.sync_loop_handle = Some(tokio::spawn(async move {
            loop {
                select! {
                    Some(event) = state.pod_lifecycle_event_rx.recv() => {
                        if let Err(e) = sync_pod_for_pod_lifecycle_event(&state, &event).await {
                            tracing::error!("Error syncing pod for lifecycle event {:?}: {:?}", event, e);
                        }
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

async fn sync_pod_for_pod_lifecycle_event(
    state: &State,
    event: &PodLifecycleEvent,
) -> anyhow::Result<()> {
    let status_manager = STATUS_MANAGER
        .get()
        .cloned()
        .ok_or(anyhow::anyhow!("StatusManager not initialized"))?;

    let mut pod_status = status_manager
        .get_pod_status(event.pod_uid)
        .await
        .ok_or(anyhow::anyhow!("Pod status not found"))?;

    let client = QUICClient::<Daemon>::connect(&state.server_addr, &state.tls_cfg).await?;
    let pod = get_pod_by_uid(&client, &event.pod_uid)
        .await?
        .ok_or(anyhow::anyhow!("Pod not found"))?;

    apply_pod_lifecycle_event(&mut pod_status, event);

    status_manager.set_pod_status(&pod, &pod_status).await?;
    Ok(())
}

fn apply_pod_lifecycle_event(pod_status: &mut PodStatus, event: &PodLifecycleEvent) {
    if pod_status.conditions.is_none() {
        pod_status.conditions = Some(Vec::new());
    }

    let container = &event.container;
    match event.event_type {
        PodLifecycleEventType::ContainerCreating => {
            tracing::info!(
                "Handling ContainerCreating event for pod {:?}",
                event.pod_uid
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
            tracing::info!(
                "Handling ContainerStarted event for pod {:?}",
                event.pod_uid
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
            tracing::info!("Handling ContainerDied event for pod {:?}", event.pod_uid);

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
        }
        _ => {
            tracing::warn!("Unhandled PodLifecycleEventType: {:?}", event.event_type);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
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

    #[test]
    fn apply_event_container_creating_sets_pending_and_condition() {
        let mut pod_status = PodStatus::default();
        let container = make_container("c1", None);
        let event = make_event(PodLifecycleEventType::ContainerCreating, container);

        apply_pod_lifecycle_event(&mut pod_status, &event);

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

    #[test]
    fn apply_event_container_started_sets_running_state() {
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

        apply_pod_lifecycle_event(&mut pod_status, &event);

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

    #[test]
    fn apply_event_container_died_succeeds_when_all_terminated() {
        let created_at = Utc::now();
        let container = make_container("c1", Some(created_at));
        let event = make_event(PodLifecycleEventType::ContainerDied, container);

        let mut pod_status = PodStatus::default();
        pod_status.phase = Some(PodPhase::Running);

        apply_pod_lifecycle_event(&mut pod_status, &event);

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

    #[test]
    fn apply_event_container_died_does_not_complete_with_running_container() {
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

        apply_pod_lifecycle_event(&mut pod_status, &event);

        assert_eq!(pod_status.phase, Some(PodPhase::Running));
    }
}
